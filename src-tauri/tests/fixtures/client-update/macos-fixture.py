#!/usr/bin/env python3
"""Disposable signed app archive. No production app, home or identity is opened."""
import contextlib
import importlib.util
import json
from pathlib import Path
import plistlib
import subprocess
import sys
import shutil
import tarfile

root = Path(__file__).resolve().parents[4]
directory = Path(sys.argv[1]).resolve()
if not directory.is_relative_to(Path('/private/tmp')) and not directory.is_relative_to(Path('/private/var/folders')):
    raise SystemExit('Fixture output must be disposable system temporary storage')
spec = importlib.util.spec_from_file_location('fixture_signing', root / 'scripts/macos-local-signing.py')
signing = importlib.util.module_from_spec(spec)
spec.loader.exec_module(signing)

def run(args):
    result = subprocess.run([str(arg) for arg in args], capture_output=True, text=True)
    if result.returncode:
        raise RuntimeError(f'{Path(args[0]).name} failed: {result.stderr}')
    return result.stdout

@contextlib.contextmanager
def fixture_identity(name):
    location = directory / name
    try:
        with signing.identity(location, initialize=True) as stored:
            yield stored
    finally:
        keychain = location / 'signing.keychain-db'
        if keychain.exists():
            run(['/usr/bin/security', 'delete-keychain', keychain])

with fixture_identity('identity') as (keychain, digest):
    for place, version in [('installed', '1.0.0'), ('incoming', '1.1.0')]:
        app = directory / place / 'Token Market.app'
        executable = app / 'Contents/MacOS/Token Market'
        executable.parent.mkdir(parents=True)
        source = directory / f'{place}.c'
        source.write_text('#include <stdio.h>\nint main(void) { puts("' + version + '"); return 0; }\n')
        run(['/usr/bin/clang', '-arch', 'arm64', source, '-o', executable])
        (app / 'Contents/Info.plist').write_bytes(plistlib.dumps({
            'CFBundleIdentifier': 'cn.tokenmarket.setup', 'CFBundleExecutable': 'Token Market',
            'CFBundleName': 'Token Market', 'CFBundleShortVersionString': version,
            'CFBundleVersion': version, 'CFBundlePackageType': 'APPL',
        }))
        signing.sign(app, 'cn.tokenmarket.setup', keychain, digest, 'production')
    for place, filename in [('installed', 'installed.tar.gz'), ('incoming', 'update.tar.gz')]:
        with tarfile.open(directory / filename, 'w:gz') as archive:
            archive.add(directory / place / 'Token Market.app', arcname='Token Market.app')
# A separately signed, otherwise matching archive must fail identity continuity.
changed = directory / 'changed/Token Market.app'
shutil.copytree(directory / 'incoming/Token Market.app', changed)
with fixture_identity('changed-identity') as (keychain, digest):
    signing.sign(changed, 'cn.tokenmarket.setup', keychain, digest, 'production')
with tarfile.open(directory / 'changed-identity.tar.gz', 'w:gz') as archive:
    archive.add(changed, arcname='Token Market.app')
cli = Path(run(['node', '-p', "require.resolve('@tauri-apps/cli/package.json')"]).strip()).parent / 'tauri.js'
run(['node', cli, 'signer', 'generate', '-w', directory / 'updater.key', '-p', ''])
for filename in ['update.tar.gz', 'changed-identity.tar.gz']:
    run(['node', cli, 'signer', 'sign', '-f', directory / 'updater.key', '-p', '', '--app-version', '1.1.0', directory / filename])
(directory / 'updater.key').unlink()
