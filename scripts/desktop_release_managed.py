"""Build/stage/publish the current local Mac + Windows NSIS release workflow.

The old native/candidate helpers remain the lower-level signing/acceptance seam.
This module owns the nine-file website + updater layout used by current releases.
"""
from contextlib import ExitStack, contextmanager
import configparser
from datetime import datetime, timezone
import hashlib
import importlib.util
import json
import os
from pathlib import Path
import platform
import re
import shutil
import struct
import subprocess
import sys
import tempfile
from urllib.error import HTTPError
from urllib.parse import urlsplit
from urllib.request import HTTPRedirectHandler, ProxyHandler, Request, build_opener

import desktop_update_local as local

ROOT = Path(__file__).resolve().parents[1]
SPEC = importlib.util.spec_from_file_location('desktop_release_api', ROOT / 'scripts/desktop-update-release.py')
api = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(api)
require, digest = api.require, api.digest
MAC, WIN, ARCHIVE = 'TokenMarket-mac-arm64.dmg', 'TokenMarket-windows-x64.exe', 'TokenMarket-mac-arm64.app.tar.gz'
DISCOVERY, IMMUTABLE = api.DISCOVERY, api.IMMUTABLE
VERSION = r'(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)'


def read_json(path):
    return json.loads(Path(path).read_text(encoding='utf-8'))


def final_file(directory, name):
    require(isinstance(name, str) and Path(name).name == name, 'Prepared artifact must use a leaf filename')
    path = directory / name
    require(path.is_file() and not path.is_symlink() and directory in path.resolve().parents, 'Prepared artifact missing or escaped its directory')
    return path


def child(command, *, env=None, stdin=None, timeout=300, label='Release tool'):
    if env is None:
        env = clean_environment()
    return api.child(command, env=env, stdin=stdin, timeout=timeout, label=label)


def clean_environment():
    return {k: v for k, v in os.environ.items() if not k.startswith(
        ('OSS_', 'OSSUTIL_', 'TAURI_SIGNING_', 'TOKEN_MARKET_UPDATER_', 'ALIBABA_CLOUD_', 'ALICLOUD_', 'GITHUB_'))}


def config_at(path, fixture=False):
    path = local.external(str(path), 'config')
    config = read_json(path)
    require(config.get('schema_version') == 1, 'Unsupported private config schema')
    parsed = urlsplit(api.public_base(config['public_base_url'], fixture))
    if fixture:
        require(parsed.scheme == 'http' and parsed.hostname in ('127.0.0.1', 'localhost'), 'Fixture public routes must use isolated loopback HTTP')
    require(re.fullmatch(r'[A-Za-z0-9.-]+', parsed.hostname or ''), 'Invalid public hostname')
    require(re.fullmatch(r'[A-Za-z0-9.-]+(?::[0-9]{1,5})?', parsed.netloc)
            and (parsed.port is None or 1 <= parsed.port <= 65535), 'Invalid public authority')
    require(parsed.path.strip('/') == config['oss']['prefix'], 'Public base path must match OSS prefix')
    api.safe_key(config['oss']['prefix'])
    api.safe_key(config['manifest_key'])
    require(config['manifest_key'].endswith('stable.json') and not config['manifest_key'].startswith(('test/', 'releases/')),
            'Expected production stable manifest key')
    for key in ('public_key_file', 'private_key_file', 'private_key_password_file', 'output_dir'):
        if config.get(key):
            config[key] = str(local.external(config[key], key))
    if config['oss'].get('config_file'):
        config['oss']['config_file'] = str(local.external(config['oss']['config_file'], 'oss.config_file'))
    config['_config_path'] = str(path)
    config['_origin'] = parsed.scheme + '://' + parsed.netloc
    config['_manifest_key'] = parsed.path.strip('/') + '/' + config['manifest_key']
    require(Path(config['public_key_file']).is_file(), 'Missing public_key_file')
    return config


def metadata(key, manifest_key):
    kind = ('text/html; charset=utf-8' if key == 'download' else 'application/json' if key == manifest_key
            else 'text/plain' if key.endswith('.sig') else 'application/gzip' if key.endswith('.tar.gz')
            else 'application/octet-stream')
    return dict(content_type=kind, content_disposition='inline' if key in ('download', manifest_key)
        else 'attachment; filename="' + Path(key).name + '"',
        cache_control=IMMUTABLE if key.startswith('releases/') else DISCOVERY)


def artifact_probe(config, version, objects):
    return dict(version=version, public_key=Path(config['public_key_file']).read_text().strip(),
                required_targets=2, objects=objects)


def verify_artifacts(config, probe):
    api.verify(probe, config['tools']['verifier'])


def stage(args, config):
    macdir = local.external(str(Path(args.mac_dir).resolve()), 'mac-dir')
    windir = local.external(str(Path(args.windows_dir).resolve()), 'windows-dir')
    output = local.external(str(Path(args.output).resolve()), 'output')
    require(not output.exists(), 'Existing staging directory preserved; choose a new output')
    mac, macbuild = read_json(macdir / 'prepared.json'), read_json(macdir / 'build.json')
    if (windir / 'prepared.json').is_file():
        windows, winbuild = read_json(windir / 'prepared.json'), read_json(windir / 'build.json')
        require(windows['target'] == 'windows-x86_64-nsis', 'Windows update target must be NSIS')
        winfile, signature = final_file(windir, windows['artifact']), windows['signature']
        version = windows['version']
        code_signed = windows['platform_signing'].get('kind') == 'authenticode' and windows['platform_signing'].get('verified') is True
    else:
        winbuild = read_json(windir / 'cross-build-verification.json')
        candidates = list(windir.glob('*.exe'))
        require(len(candidates) == 1, 'Exactly one finalized cross-built NSIS installer required')
        winfile = candidates[0]
        require(not winfile.is_symlink(), 'Windows installer must not be a symlink')
        require(digest(winfile) == winbuild['artifact_sha256'], 'Cross-built Windows bytes differ from record')
        signature = winfile.with_name(winfile.name + '.sig').read_text().strip()
        version, code_signed = winbuild['version'], winbuild.get('authenticode_signed') is True
    fixture = bool(getattr(args, 'fixture', False))
    require(mac.get('fixture', False) is fixture and macbuild.get('fixture', False) is fixture
            and winbuild.get('fixture', False) is fixture, 'Fixture and production build records cannot be mixed')
    require(mac['version'] == version and re.fullmatch(VERSION, version), 'Platform versions must match')
    require(mac['target'] == 'darwin-aarch64' and macbuild['version'] == version, 'Mac target/version mismatch')
    require(winbuild['version'] == version and winfile.suffix.lower() == '.exe', 'Windows record/installer version or format mismatch')
    require(macbuild['source_commit'] == winbuild['source_commit'] and re.fullmatch('[a-f0-9]{40}', macbuild['source_commit']),
            'Both platforms must use the same full source commit')
    macfile = final_file(macdir, mac['artifact'])
    require(digest(macfile) == mac['platform_signing']['artifact_sha256'], 'Mac update bytes changed')
    manuals = [v for v in mac['installers'] if v['path'].endswith('.dmg')]
    require(len(manuals) == 1 and digest(final_file(macdir, manuals[0]['path'])) == manuals[0]['platform_signing']['artifact_sha256'],
            'Mac manual installer bytes changed')
    if not fixture:
        fingerprint = hashlib.sha256(Path(config['public_key_file']).read_text().strip().encode()).hexdigest()
        require(macbuild.get('compiled', {}).get('public_key_sha256') == fingerprint
                and winbuild.get('compiled', {}).get('public_key_sha256', winbuild.get('signing_public_key_sha256')) == fingerprint,
                'Compiled updater public key differs from release signing key')
        require(macbuild.get('prepared_sha256') == digest(macdir / 'prepared.json'), 'Mac build/prepared binding differs')
        if (windir / 'prepared.json').is_file():
            require(winbuild.get('prepared_sha256') == digest(windir / 'prepared.json'), 'Windows build/prepared binding differs')
    # Verify before creating any public output. No signing private key is read here.
    verify_artifacts(config, artifact_probe(config, version, [
        dict(role='updater', platform='darwin-aarch64', path=str(macfile), signature=mac['signature']),
        dict(role='updater', platform='windows-x86_64-nsis', path=str(winfile), signature=signature)]))
    notes = Path(args.notes_file).read_text(encoding='utf-8') if args.notes_file else 'Token Market ' + version + ' 稳定版。'
    output.parent.mkdir(parents=True, exist_ok=True)
    with tempfile.TemporaryDirectory(prefix='.release-stage-', dir=output.parent) as temporary:
        folder = Path(temporary)
        public, versioned = folder / 'public', folder / 'public/releases' / version
        versioned.mkdir(parents=True)
        for source, name in [(macdir / manuals[0]['path'], MAC), (macfile, ARCHIVE), (winfile, WIN)]:
            shutil.copy2(source, versioned / name)
            if name in (MAC, WIN):
                shutil.copy2(source, public / name)
        (versioned / (ARCHIVE + '.sig')).write_text(mac['signature'].strip(), encoding='utf-8')
        (versioned / (WIN + '.sig')).write_text(signature.strip(), encoding='utf-8')
        template = (ROOT / 'scripts/release-assets/download.html').read_text(encoding='utf-8')
        (public / 'download').write_text(template.replace('__DOWNLOAD_ORIGIN__', config['_origin']), encoding='utf-8')
        prefix = config['_origin'] + '/releases/' + version + '/'
        manifest = dict(version=version, notes=notes, platforms={
            'darwin-aarch64': dict(url=prefix + ARCHIVE, signature=mac['signature'].strip()),
            'windows-x86_64-nsis': dict(url=prefix + WIN, signature=signature.strip())})
        file = public / config['_manifest_key']
        file.parent.mkdir(parents=True, exist_ok=True)
        file.write_text(json.dumps(manifest, ensure_ascii=False, indent=2) + '\n', encoding='utf-8')
        objects = []
        for file in sorted(public.rglob('*')):
            if file.is_file():
                key = str(file.relative_to(public))
                objects.append(dict(object_key=key, url=config['_origin'] + '/' + key,
                    sha256=digest(file), size=file.stat().st_size, **metadata(key, config['_manifest_key'])))
        plan = dict(schema_version=1, fixture=fixture, version=version, source_commit=macbuild['source_commit'],
            objects=objects, signing_public_key_sha256=hashlib.sha256(Path(config['public_key_file']).read_text().strip().encode()).hexdigest(),
            windows_authenticode_signed=code_signed, macos_notarized=mac['platform_signing'].get('notarized') is True,
            native_device_validated=False, uploaded=False)
        (folder / 'upload-map.json').write_text(json.dumps(plan, ensure_ascii=False, indent=2) + '\n', encoding='utf-8')
        os.rename(folder, output)
    return output / 'upload-map.json'


def load_plan(args, config):
    path = local.external(str(Path(args.plan).resolve()), 'plan')
    plan = read_json(path)
    require(plan.get('schema_version') == 1 and plan.get('fixture') is args.fixture, 'Plan schema/fixture mismatch; restage finalized outputs')
    version = plan['version']
    require(isinstance(version, str) and re.fullmatch(VERSION, version), 'Stable semantic version required')
    prefix, manifest_key = 'releases/' + version + '/', config['_manifest_key']
    allowed = {MAC, WIN, 'download', manifest_key} | {prefix + v for v in (MAC, WIN, ARCHIVE, WIN + '.sig', ARCHIVE + '.sig')}
    objects = plan['objects']
    require(len(objects) == len(allowed) and {v['object_key'] for v in objects} == allowed, 'Plan must contain exactly the nine approved public objects')
    public = (path.parent / 'public').resolve()
    for obj in objects:
        key, file = obj['object_key'], public / obj['object_key']
        require(file.is_file() and not file.is_symlink() and public in file.resolve().parents, 'Public file missing or escaped staging directory: ' + key)
        require(file.stat().st_size == obj['size'] and digest(file) == obj['sha256'], 'Staged file bytes changed: ' + key)
        require(obj['url'] == config['_origin'] + '/' + key, 'Public URL mismatch: ' + key)
        obj.update(metadata(key, manifest_key), path=str(file))
    manifest = read_json(public / manifest_key)
    require(manifest['version'] == version and set(manifest['platforms']) == set(api.TARGETS), 'Manifest version/platform mismatch')
    probes = []
    for target, name in [('darwin-aarch64', ARCHIVE), ('windows-x86_64-nsis', WIN)]:
        require(manifest['platforms'][target]['url'] == config['_origin'] + '/' + prefix + name, 'Manifest must point to the immutable version package')
        probes.append(dict(role='updater', platform=target, path=str(public / prefix / name), signature=manifest['platforms'][target]['signature']))
    verify_artifacts(config, artifact_probe(config, version, probes))
    require(plan.get('signing_public_key_sha256') == hashlib.sha256(Path(config['public_key_file']).read_text().strip().encode()).hexdigest(),
            'Plan updater public key mismatch')
    plan.update(manifest=manifest, public=str(public), path=str(path))
    return plan


class NoRedirect(HTTPRedirectHandler):
    def redirect_request(self, *args, **kwargs):
        return None


def remote(url, limit=128 * 1024 * 1024):
    try:
        with build_opener(ProxyHandler({}), NoRedirect()).open(Request(url,
                headers={'Accept-Encoding': 'identity', 'Cache-Control': 'no-cache'}), timeout=45) as response:
            data = response.read(limit + 1)
            require(len(data) <= limit, 'Public object exceeds expected size limit')
            return data, response.headers
    except HTTPError as error:
        if error.code == 404:
            return None
        raise ValueError('Public request failed with HTTP ' + str(error.code)) from None


def verify_remote(obj, manifest_key, *, headers=True):
    result = remote(obj['url'], obj['size'])
    require(result is not None, 'Public object missing: ' + obj['object_key'])
    data, meta = result
    require(len(data) == obj['size'] and hashlib.sha256(data).hexdigest() == obj['sha256'], 'Public object bytes differ: ' + obj['object_key'])
    if headers:
        require(meta.get_content_type() == obj['content_type'].split(';')[0] and not meta.get('Content-Encoding'), 'Public content headers differ: ' + obj['object_key'])
        cache = {v.strip().lower() for v in meta.get('Cache-Control', '').split(',')}
        require(all(v.strip().lower() in cache for v in obj['cache_control'].split(',')), 'Public cache headers differ: ' + obj['object_key'])
        if obj['object_key'] == 'download':
            require('attachment' not in meta.get('Content-Disposition', '').lower() and meta.get('x-oss-force-download', '').lower() != 'true', 'Download page forced to attachment')
        if obj['object_key'] in (MAC, WIN):
            require(obj['object_key'] in meta.get('Content-Disposition', ''), 'Fixed download filename header differs')


def credentials(config, fixture):
    if fixture:
        return
    oss, parser = config['oss'], configparser.ConfigParser(interpolation=None)
    parser.read(oss['config_file'])
    section = 'profile ' + oss['profile']
    require(all(parser.has_option(section, k) and parser.get(section, k).strip() for k in ('access-key-id', 'access-key-secret')), 'Missing local OSS profile credentials')
    require(child([oss['ossutil'], 'version']).strip() == '2.4.1', 'ossutil2.4.1 required')


def put(args, config, obj, *, metadata_only=False):
    if args.fixture:
        store = local.external(str(Path(args.local_store).resolve()), 'fixture store')
        file = store / obj['object_key']
        require(store in file.resolve().parents, 'Unsafe fixture object path')
        file.parent.mkdir(parents=True, exist_ok=True)
        if not metadata_only:
            shutil.copyfile(obj['path'], file)
        values = read_json(store / 'metadata.json') if (store / 'metadata.json').is_file() else {}
        values[obj['object_key']] = metadata(obj['object_key'], config['_manifest_key'])
        (store / 'metadata.json').write_text(json.dumps(values), encoding='utf-8')
        with (store / 'operations.jsonl').open('a') as stream:
            stream.write(json.dumps(dict(key=obj['object_key'], metadata_only=metadata_only)) + '\n')
        return
    oss = config['oss']
    command = [oss['ossutil'], 'set-props' if metadata_only else 'cp']
    if not metadata_only:
        command.append(obj['path'])
    command += ['oss://' + oss['bucket'] + '/' + obj['object_key'], '--config-file', oss['config_file'], '--profile', oss['profile'],
        '--region', oss['region'], '--endpoint', oss['endpoint'], '--acl', 'public-read', '--cache-control', obj['cache_control'],
        '--content-type', obj['content_type'], '--content-disposition', obj['content_disposition'], '--force', '--loglevel', 'off']
    if metadata_only:
        command += ['--metadata-directive', 'update']
    child(command, label='OSS metadata update' if metadata_only else 'OSS object upload')


@contextmanager
def publication_lock(args, config):
    # Fixed website aliases live at bucket root, so differing manifest prefixes
    # must still serialize. Also retain exclusion with the older prefix publisher.
    prefixes = [config['oss']['prefix']] if args.fixture else sorted({config['oss']['prefix'], 'managed-root-public'})
    with ExitStack() as stack:
        for prefix in prefixes:
            destination = dict(config, oss=dict(config['oss'], prefix=prefix))
            stack.enter_context(local.publisher_lock(destination, args, api))
        yield


def publish(args, config, plan):
    credentials(config, args.fixture)
    with publication_lock(args, config):
        current = remote(config['_origin'] + '/' + config['_manifest_key'], 1024 * 1024)
        if current:
            previous = json.loads(current[0])
            require(set(previous.get('platforms', {})) == set(api.TARGETS) and re.fullmatch(VERSION, previous.get('version', ''))
                    and isinstance(previous.get('notes'), str)
                    and all(isinstance(item.get('url'), str) and item['url'] and isinstance(item.get('signature'), str) and item['signature']
                            for item in previous['platforms'].values()), 'Existing manifest is malformed')
            require(tuple(map(int, previous['version'].split('.'))) <= tuple(map(int, plan['version'].split('.'))), 'A newer stable version is already published')
        if args.command == 'metadata':
            keys = set(args.key or [v['object_key'] for v in plan['objects']])
            require(keys <= {v['object_key'] for v in plan['objects']}, 'Unknown metadata key')
            for obj in plan['objects']:
                if obj['object_key'] in keys:
                    verify_remote(obj, config['_manifest_key'], headers=False)
                    put(args, config, obj, metadata_only=True)
                    verify_remote(obj, config['_manifest_key'])
            return
        ordered = sorted(plan['objects'], key=lambda o: (2 if o['object_key'] == config['_manifest_key'] else 0 if o['object_key'].startswith('releases/') else 1, o['object_key']))
        for obj in ordered:
            existing = remote(obj['url'], obj['size']) if obj['object_key'].startswith('releases/') else None
            if existing:
                require(hashlib.sha256(existing[0]).hexdigest() == obj['sha256'], 'Immutable version object already has different bytes: ' + obj['object_key'])
            try:
                put(args, config, obj, metadata_only=bool(existing))
                verify_remote(obj, config['_manifest_key'])
            except (ValueError, OSError, subprocess.SubprocessError):
                if obj['object_key'] == config['_manifest_key']:
                    raise ValueError('stable.json publication or public verification failed; live discovery may have advanced. Inspect it before retrying.') from None
                raise


def check(config, plan):
    for obj in plan['objects']:
        verify_remote(obj, config['_manifest_key'])
    result = json.loads(child([config['tools']['verifier']], stdin=json.dumps(dict(mode='manifest',
        manifest=plan['manifest'], public_key=Path(config['public_key_file']).read_text().strip())), label='Native manifest parser verification'))
    require(result.get('verified') is True and len(result.get('checks', [])) == 4, 'Native manifest parser did not confirm both platform targets')


def compiler_environment(config, cache):
    env = clean_environment()
    for key in list(env):
        if key.startswith(('TAURI_', 'APPLE_', 'CSC_', 'CARGO_TARGET_DIR')):
            del env[key]
    llvm = next((p for p in (Path('/opt/homebrew/opt/llvm/bin'), Path('/usr/local/opt/llvm/bin')) if (p / 'llvm-rc').is_file()), None)
    if llvm:
        env['PATH'] = str(llvm) + os.pathsep + env['PATH']
    env.update(CARGO_TARGET_DIR=str(cache / 'cargo-target'), XWIN_CACHE_DIR=str(cache / 'xwin-cache'),
        TOKEN_MARKET_UPDATER_ENDPOINT=config['public_base_url'].rstrip('/') + '/' + config['manifest_key'],
        TOKEN_MARKET_UPDATER_RECOVERY_URL=config['public_base_url'].rstrip('/') + '/' + config.get('recovery_key', 'recovery/index.html'),
        TOKEN_MARKET_UPDATER_PUBLIC_KEY=Path(config['public_key_file']).read_text().strip(),
        TOKEN_MARKET_BUILD_CONSOLE_API_BASE_URL='https://console-api.tokensapi.cn',
        TOKEN_MARKET_BUILD_GATEWAY_API_BASE_URL='https://api.tokensapi.cn')
    return env


def pe_info(path):
    data = path.read_bytes()
    require(data[:2] == b'MZ' and len(data) >= 64, 'Expected PE file')
    offset = struct.unpack_from('<I', data, 0x3c)[0]
    require(data[offset:offset + 4] == b'PE\0\0', 'Invalid PE header')
    optional = offset + 24
    magic = struct.unpack_from('<H', data, optional)[0]
    require(magic in (0x20b, 0x10b), 'Invalid PE optional header')
    directory = optional + (112 if magic == 0x20b else 96)
    cert = struct.unpack_from('<II', data, directory + 32)
    return dict(machine=struct.unpack_from('<H', data, offset + 4)[0], authenticode_certificate_table_present=bool(cert[0] and cert[1]))


def build_windows(config, source, commit, cache):
    output = local.external(str(Path(config['output_dir']) / config['version'] / 'windows-x86_64-cross'), 'Windows output')
    require(not output.exists(), 'Windows output already exists; use stage/publish to resume finalized outputs')
    env = compiler_environment(config, cache)
    require(not config['native'].get('windows_certificate_sha1'), 'Windows certificate configured: use native Windows build and pass --windows-dir; do not silently skip code signing')
    for executable in ('cargo-xwin', 'llvm-rc', 'lld-link', 'makensis', '7zz'):
        require(shutil.which(executable, path=env['PATH']), 'Missing cross-build tool: ' + executable)
    require(child([config['tools']['signer'], '--version']).strip() == 'tauri-cli 2.12.4', 'Tauri CLI2.12.4 required')
    # This follows the official Tauri NSIS cross-build path, not the native Authenticode recipe.
    process = subprocess.run([config['tools']['signer'], 'build', '--target', 'x86_64-pc-windows-msvc', '--runner', 'cargo-xwin',
        '--bundles', 'nsis', '--ci', '--', '--locked'], cwd=source, env=env, capture_output=True, timeout=7200)
    output.parent.mkdir(parents=True, exist_ok=True)
    log = output.parent / 'windows-cross-build.log'
    log.write_bytes(process.stdout + process.stderr)
    log.chmod(0o600)
    require(process.returncode == 0, 'Windows NSIS cross-build failed; inspect private log: ' + str(log))
    release = Path(env['CARGO_TARGET_DIR']) / 'x86_64-pc-windows-msvc/release'
    # The shared cache also contains older releases. Only the requested version
    # may supply this release's installer; never pick the newest arbitrary EXE.
    candidates = list((release / 'bundle/nsis').glob('*_' + config['version'] + '_x64-setup.exe'))
    require(len(candidates) == 1, 'Expected exactly one final NSIS installer')
    installer, app = candidates[0], release / 'app.exe'
    child([shutil.which('7zz', path=env['PATH']), 't', installer], label='NSIS archive integrity check')
    output.parent.mkdir(parents=True, exist_ok=True)
    with tempfile.TemporaryDirectory(prefix='.windows-prepare-', dir=output.parent) as temporary:
        folder = Path(temporary)
        extracted = folder / 'extracted'
        child([shutil.which('7zz', path=env['PATH']), 'x', '-y', installer, '-o' + str(extracted)], label='NSIS payload extraction')
        payload = extracted / 'app.exe'
        baseline, actual = app.read_bytes(), payload.read_bytes()
        marker = b'__TAURI_BUNDLE_TYPE_VAR_UNK'
        require(baseline == actual or baseline.count(marker) == 1 and baseline.replace(marker, b'__TAURI_BUNDLE_TYPE_VAR_NSS', 1) == actual,
                'Installer payload differs beyond the official NSIS bundle marker')
        info = pe_info(payload)
        require(info['machine'] == 0x8664 and not info['authenticode_certificate_table_present'] and not pe_info(installer)['authenticode_certificate_table_present'], 'Unexpected PE architecture/signature state')
        for value in (env['TOKEN_MARKET_UPDATER_ENDPOINT'], env['TOKEN_MARKET_UPDATER_RECOVERY_URL'], env['TOKEN_MARKET_UPDATER_PUBLIC_KEY'],
                      env['TOKEN_MARKET_BUILD_CONSOLE_API_BASE_URL'], env['TOKEN_MARKET_BUILD_GATEWAY_API_BASE_URL']):
            require(value.encode() in actual, 'Compiled Windows updater configuration missing')
        recipe = (release / 'nsis/x64/installer.nsi').read_text()
        require('!define INSTALLMODE "currentUser"' in recipe and '!define VERSION "' + config['version'] + '"' in recipe
                and 'windows/update-hooks.nsh' in recipe and '!insertmacro NSIS_HOOK_POSTINSTALL' in recipe, 'NSIS current-user lifecycle contract missing')
        final = folder / WIN
        shutil.copy2(installer, final)
        signing = dict(env, TAURI_SIGNING_PRIVATE_KEY=Path(config['private_key_file']).read_text().strip(),
            TAURI_SIGNING_PRIVATE_KEY_PASSWORD=Path(config['private_key_password_file']).read_text().rstrip('\r\n') if config.get('private_key_password_file') else '')
        child([config['tools']['signer'], 'signer', 'sign', '--app-version', config['version'], final], env=signing, label='Windows updater file signing')
        record = dict(version=config['version'], source_commit=commit, fixture=False, provenance_kind='macos-cross-build-verification',
            completed_at=datetime.now(timezone.utc).isoformat(),
            artifact_sha256=digest(final), artifact_size=final.stat().st_size, payload_sha256=digest(payload),
            authenticode_signed=False, native_device_validated=False, target='windows-x86_64-nsis', install_mode='currentUser',
            compiled=local.compiled(dict(config, public_key=env['TOKEN_MARKET_UPDATER_PUBLIC_KEY'])))
        (folder / 'cross-build-verification.json').write_text(json.dumps(record, indent=2) + '\n', encoding='utf-8')
        shutil.rmtree(extracted)
        require(local.checked_source(config, api)[1] == commit, 'Source changed during Windows build')
        os.rename(folder, output)
    return output


def build(args, config):
    require(sys.platform == 'darwin' and platform.machine().lower() in ('arm64', 'aarch64'), 'Managed dual-platform build requires Apple Silicon macOS')
    source = Path(args.source or config['source_root']).resolve()
    settings = {k: v for k, v in config.items() if not k.startswith('_')}
    settings.update(source_root=str(source), version=args.version or config['version'])
    _, commit = local.checked_source(settings, api)
    require(source == ROOT, 'Run the build command from the same clean source checkout; use --source only to confirm its identity')
    local.external(settings['output_dir'], 'output_dir')
    parent = Path(config['_config_path']).parent
    # Keep the working first-release cache in place; moving an SDK can invalidate
    # embedded compiler paths. Fresh installations use build-cache/.
    cache = local.external(str(parent / ('windows-cross' if (parent / 'windows-cross/xwin-cache').is_dir() else 'build-cache')), 'build cache')
    cache.mkdir(parents=True, exist_ok=True)
    # All managed builds on this account serialize even across source checkouts.
    guard_config = dict(config, oss=dict(config['oss'], prefix='build-cache-' + hashlib.sha256(str(cache).encode()).hexdigest()))
    lock_args = type('LockArgs', (), {'fixture': False, 'local_store': None})()
    with local.publisher_lock(guard_config, lock_args, api):
        private = Path(settings['output_dir']) / settings['version']
        private.mkdir(parents=True, exist_ok=True)
        snapshot = private / 'config.json'
        snapshot.write_text(json.dumps(settings, ensure_ascii=False, indent=2) + '\n', encoding='utf-8')
        snapshot.chmod(0o600)
        target = getattr(args, 'target', 'all')
        mac, windows = None, None
        if target in ('all', 'mac'):
            mac = private / 'darwin-aarch64'
            if mac.exists():
                previous = read_json(mac / 'build.json')
                require(previous['source_commit'] == commit and previous['version'] == settings['version']
                        and previous['compiled'] == local.compiled(dict(settings, public_key=Path(settings['public_key_file']).read_text().strip())),
                        'Existing Mac output has a different source/configuration; preserve it and choose a new version')
            else:
                native_env = clean_environment()
                native_env.update(TOKEN_MARKET_BUILD_CONSOLE_API_BASE_URL='https://console-api.tokensapi.cn', TOKEN_MARKET_BUILD_GATEWAY_API_BASE_URL='https://api.tokensapi.cn')
                child([sys.executable, source / 'scripts/desktop-update-release.py', 'local-build', '--config', snapshot, '--target', 'darwin-aarch64'],
                      env=native_env, timeout=7200, label='Mac native build/sign')
        if target in ('all', 'windows'):
            windows = Path(args.windows_dir).resolve() if args.windows_dir else private / 'windows-x86_64-cross'
            if args.windows_dir:
                require(windows.is_dir(), 'Missing finalized native Windows directory')
            elif windows.exists():
                previous = read_json(windows / 'cross-build-verification.json')
                require(previous['source_commit'] == commit and previous['version'] == settings['version']
                        and previous.get('compiled') == local.compiled(dict(settings, public_key=Path(settings['public_key_file']).read_text().strip())),
                        'Existing Windows output has a different source/configuration; preserve it and choose a new version')
            else:
                windows = build_windows(settings, source, commit, cache)
        require(local.checked_source(settings, api)[1] == commit, 'Source changed during managed build')
    return mac, windows
