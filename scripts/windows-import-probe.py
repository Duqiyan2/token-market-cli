"""Temporary hosted Windows diagnostic; standard library only, no settings writes.
Run as Cargo's runner to inherit the same DLL search PATH as the failed test.
"""
import ctypes
import json
import os
from pathlib import Path
import struct
import subprocess
import sys

PREFIX = '[DEBUG-ci-loader-20261105]'

def say(message):
    print(PREFIX, message, flush=True)


def imports(path):
    data = path.read_bytes()
    u16 = lambda offset: struct.unpack_from('<H', data, offset)[0]
    u32 = lambda offset: struct.unpack_from('<I', data, offset)[0]
    pe = u32(0x3c)
    if data[pe:pe+4] != b'PE\0\0':
        raise ValueError('Not a PE image')
    optional = pe + 24
    plus = u16(optional) == 0x20b
    width = 8 if plus else 4
    directories = optional + (112 if plus else 96)
    sections = optional + u16(pe + 20)
    entries = []
    for index in range(u16(pe + 6)):
        base = sections + 40 * index
        entries.append((u32(base+12), max(u32(base+8), u32(base+16)), u32(base+20)))
    def offset(rva):
        for start, size, raw in entries:
            if start <= rva < start + size:
                return raw + rva - start
        if rva < u32(optional + 60):
            return rva
        raise ValueError('Unmapped RVA: ' + hex(rva))
    def string(rva):
        start = offset(rva)
        return data[start:data.index(b'\0', start)].decode('ascii')
    directory_rva = u32(directories + 8)
    if not directory_rva:
        return
    descriptor = offset(directory_rva)
    while any(data[descriptor:descriptor+20]):
        original, _, _, name, first = struct.unpack_from('<IIIII', data, descriptor)
        thunk = offset(original or first)
        names = []
        while True:
            value = struct.unpack_from('<Q' if plus else '<I', data, thunk)[0]
            if not value:
                break
            names.append(value & 0xffff if value & (1 << (width*8-1)) else string(value+2))
            thunk += width
        yield string(name), names
        descriptor += 20


def probe(executable):
    kernel = ctypes.WinDLL('kernel32', use_last_error=True)
    load = kernel.LoadLibraryW
    load.argtypes = [ctypes.c_wchar_p]
    load.restype = ctypes.c_void_p
    address = kernel.GetProcAddress
    address.argtypes = [ctypes.c_void_p, ctypes.c_void_p]
    address.restype = ctypes.c_void_p
    filename = kernel.GetModuleFileNameW
    filename.argtypes = [ctypes.c_void_p, ctypes.c_wchar_p, ctypes.c_uint32]
    filename.restype = ctypes.c_uint32
    free = kernel.FreeLibrary
    free.argtypes = [ctypes.c_void_p]
    free.restype = ctypes.c_int
    say('exe=' + str(executable))
    say('Platform=' + sys.platform + ' Python=' + sys.version.split()[0])
    # PATH may contain unrelated environment-specific directories; do not print it.
    search = [executable.parent, Path(os.environ['SystemRoot'])/'System32', Path.cwd()]
    search += [Path(p) for p in os.environ.get('PATH', '').split(os.pathsep) if p]
    missing = []
    for dll, symbols in imports(executable):
        candidates = list(dict.fromkeys(str(p/dll) for p in search if (p/dll).is_file()))
        say('DLL=' + dll + ' candidates=' + json.dumps(candidates))
        # OS resolution in this diagnostic process is supporting evidence only;
        # Python can already have loaded a DLL. Actual test result remains primary.
        handle = load(dll)
        if not handle:
            say('LOAD_FAILED ' + dll + ' winerror=' + str(ctypes.get_last_error()))
            continue
        try:
            path = ctypes.create_unicode_buffer(32768)
            filename(handle, path, len(path))
            say('RESOLVED ' + dll + '=' + path.value + ' imports=' + str(len(symbols)))
            for symbol in symbols:
                arg = ctypes.c_void_p(symbol) if isinstance(symbol, int) else ctypes.cast(ctypes.c_char_p(symbol.encode('ascii')), ctypes.c_void_p)
                if not address(handle, arg):
                    detail = dll + '!' + str(symbol)
                    missing.append(detail)
                    say('MISSING_EXPORT ' + detail + ' winerror=' + str(ctypes.get_last_error()))
        finally:
            free(handle)
    say('Missing export count=' + str(len(missing)))


if __name__ == '__main__':
    executable = Path(sys.argv[1]).resolve()
    try:
        probe(executable)
    except Exception as error:
        say('PROBE_ERROR ' + repr(error))
    # Keep the actual failing test/loader code as the step verdict.
    result = subprocess.run([str(executable), *sys.argv[2:]])
    say('Actual test process exit=' + hex(result.returncode & 0xffffffff))
    sys.exit(result.returncode)
