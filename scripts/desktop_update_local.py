"""Private local release orchestration. Only the public release CLI calls this module."""
from contextlib import contextmanager, redirect_stdout
from urllib.error import HTTPError
import argparse
import io
import platform
import shutil
import hashlib
import json
import os
from pathlib import Path
import re
import socket
import tempfile
import sys

def external(value, field):
    if not isinstance(value, str) or not value or not Path(value).is_absolute():
        raise ValueError(field + ' requires an absolute path outside Git')
    # Resolve even nonexistent leaves: symlinked ancestors cannot bypass this rule.
    path = Path(value).resolve()
    if any((parent / '.git').exists() for parent in (path,) + tuple(path.parents)):
        raise ValueError(field + ' must be outside Git checkouts')
    return path

def load(args, api):
    path = external(args.config, 'config')
    config = json.loads(path.read_text(encoding='utf-8'))
    fields = ('schema_version', 'source_root', 'version', 'public_base_url', 'manifest_key', 'recovery_key',
              'candidate_key', 'public_key_file', 'private_key_file', 'private_key_password_file',
              'output_dir', 'tools', 'oss', 'native')
    api.object_fields(config, fields, 'Local config')
    api.require(type(config['schema_version']) is int and config['schema_version'] == 1, 'Unsupported local config schema_version')
    api.object_fields(config['tools'], ('signer', 'verifier'), 'tools')
    api.object_fields(config['oss'], ('region', 'endpoint', 'bucket', 'prefix', 'config_file', 'profile', 'ossutil'), 'oss')
    api.object_fields(config['native'], ('macos_identity_dir', 'tauri_cli_js', 'windows_certificate_sha1',
        'windows_timestamp_url', 'windows_signtool'), 'native')
    required = ['version', 'public_base_url', 'manifest_key', 'recovery_key', 'candidate_key', 'public_key_file']
    if args.command == 'local-build':
        required += ['source_root', 'private_key_file', 'output_dir']
    elif args.command == 'local-inventory':
        required += ['output_dir']
    missing = [name for name in required if not config[name]]
    if args.command in ('local-candidate', 'local-publish'):
        missing += ['oss.' + name for name in config['oss'] if not config['oss'][name]]
    missing += ['tools.' + name for name in (('signer', 'verifier') if args.command == 'local-build' else ('verifier',)) if not config['tools'][name]]
    api.require(not missing, 'Missing local configuration: ' + ', '.join(missing))
    for name in ('public_key_file', 'private_key_file', 'private_key_password_file', 'output_dir'):
        if config[name]:
            config[name] = str(external(config[name], name))
    if config['oss']['config_file']:
        config['oss']['config_file'] = str(external(config['oss']['config_file'], 'oss.config_file'))
    if config['native']['macos_identity_dir']:
        identity = external(config['native']['macos_identity_dir'], 'native.macos_identity_dir')
        config['native']['macos_identity_dir'] = str(identity)
        # The native signer consumes these concrete files, so checking only the
        # parent directory would let a child symlink bring Git material into signing.
        # Resolve paths without reading certificate, password, keychain or metadata.
        for name in ('certificate.pem', 'keychain-password', 'signing.keychain-db', 'identity.json'):
            external(str(identity / name), 'native.macos_identity_dir.' + name)
        lock = identity / 'build.lock'
        external(str(lock), 'native.macos_identity_dir.build.lock')
        # The signer opens/chmods this file. Never let that mutation follow a link,
        # even when the linked target happens to be outside Git.
        api.require(not lock.is_symlink(), 'native.macos_identity_dir.build.lock must not be a symlink')
    config['public_base_url'] = api.public_base(config['public_base_url'], args.fixture)
    for name in ('manifest_key', 'recovery_key', 'candidate_key'):
        api.safe_key(config[name])
    api.require(config['manifest_key'].endswith('stable.json') and not config['manifest_key'].startswith(('test/', 'releases/')), 'manifest_key requires a production stable route')
    api.require(config['candidate_key'].startswith('test/') and config['candidate_key'].endswith('/stable.json'), 'candidate_key requires an isolated test route')
    api.require(config['recovery_key'].endswith('.html') and not config['recovery_key'].startswith(('test/', 'releases/')), 'recovery_key requires a fixed HTML alias')
    api.require(re.fullmatch(r'(0|[1-9][0-9]*)[.](0|[1-9][0-9]*)[.](0|[1-9][0-9]*)', config['version']), 'version requires stable semantic version')
    if args.command in ('local-candidate', 'local-publish'):
        api.safe_key(config['oss']['prefix'])
        api.require(re.fullmatch(r'[a-z0-9][a-z0-9-]{1,61}[a-z0-9]', config['oss']['bucket']), 'Invalid oss.bucket')
        api.public_base(config['oss']['endpoint'], False)
    api.require(Path(config['public_key_file']).is_file(), 'Missing public_key_file')
    if args.command not in ('local-build', 'local-inventory', 'local-validate'):
        api.require(Path(config['oss']['config_file']).is_file(), 'Missing oss.config_file')
    config['public_key'] = Path(config['public_key_file']).read_text(encoding='utf-8').strip()
    api.nonempty(config['public_key'], 'public_key_file')
    args.verifier = config['tools']['verifier']
    args.ossutil = config['oss']['ossutil']
    return config


@contextmanager
def environment(config):
    # Release configuration and identity must not inherit ambient OSS/CI/signer claims.
    previous = dict(os.environ)
    try:
        for key in list(os.environ):
            if key.startswith(('OSS_', 'OSSUTIL_', 'TOKEN_MARKET_UPDATE', 'TOKEN_MARKET_UPDATER_', 'TAURI_SIGNING_', 'GITHUB_', 'ALIBABA_CLOUD_', 'ALICLOUD_')):
                del os.environ[key]
        values = dict(OSS_REGION=config['oss']['region'], OSS_ENDPOINT=config['oss']['endpoint'],
            TOKEN_MARKET_UPDATE_OSS_BUCKET=config['oss']['bucket'], TOKEN_MARKET_UPDATE_OSS_PREFIX=config['oss']['prefix'],
            TOKEN_MARKET_UPDATE_PUBLIC_BASE_URL=config['public_base_url'], OSSUTIL_CONFIG_FILE=config['oss']['config_file'],
            OSSUTIL_PROFILE=config['oss']['profile'], TOKEN_MARKET_UPDATE_PRODUCTION_MANIFEST_KEY=config['manifest_key'],
            TOKEN_MARKET_UPDATE_RECOVERY_KEY=config['recovery_key'],
            TOKEN_MARKET_UPDATER_ENDPOINT=config['public_base_url'] + '/' + config['manifest_key'],
            TOKEN_MARKET_UPDATER_RECOVERY_URL=config['public_base_url'] + '/' + config['recovery_key'],
            TOKEN_MARKET_UPDATER_PUBLIC_KEY=config['public_key'])
        os.environ.update(values)
        yield
    finally:
        os.environ.clear()
        os.environ.update(previous)


def compiled(config):
    return dict(endpoint=config['public_base_url'] + '/' + config['manifest_key'],
        recovery_url=config['public_base_url'] + '/' + config['recovery_key'],
        public_key_sha256=hashlib.sha256(config['public_key'].encode()).hexdigest())

def policy_sha(config):
    return hashlib.sha256(json.dumps(dict(version=config['version'], **compiled(config)),
        sort_keys=True, separators=(',', ':')).encode()).hexdigest()

def validate_provenance(value, config, fixture, api, label):
    api.object_fields(value, ('schema_version', 'fixture', 'provenance_kind', 'source_commit', 'version', 'target',
        'artifacts', 'prepared_sha256', 'signing_identity', 'compiled', 'native_host', 'recipe',
        'configuration_sha256', 'started_at', 'completed_at'), label + ' local build provenance')
    api.require(type(value['schema_version']) is int and value['schema_version'] == 2 and
        value['provenance_kind'] == 'local-native-build' and value['fixture'] is fixture,
        label + ' invalid local build provenance schema/classification; CI and fixture records cannot substitute')
    api.require(value['compiled'] == compiled(config) and value['configuration_sha256'] == policy_sha(config),
        label + ' local compiled configuration mismatch')
    api.nonempty(value['native_host'], label + ' native_host')
    api.require(value['recipe'] == ('fixture-adapter' if fixture else
        'build-macos-clients.sh' if value['target'] == 'darwin-aarch64' else 'build-windows-clients.ps1'),
        label + ' unrecognized native build recipe')
    try:
        start, end = [api.datetime.strptime(value[field], '%Y-%m-%dT%H:%M:%SZ').replace(tzinfo=api.timezone.utc)
            for field in ('started_at', 'completed_at')]
        api.require(start <= end <= api.datetime.now(api.timezone.utc), label + ' invalid native build interval')
    except (ValueError, TypeError):
        raise ValueError(label + ' invalid native build interval') from None

def inventory_plan(args, config, api):
    inventory = external(args.inventory, 'inventory')
    plan = api.inventory_plan(args, production_route=config['manifest_key'] if args.command != 'local-candidate' else None)
    api.require(plan['version'] == config['version'] and plan['public_key'] == config['public_key'], 'Local inventory version/public key mismatch')
    release = json.loads(inventory.read_text(encoding='utf-8'))
    for target, record in release['targets'].items():
        label = 'Local ' + target
        paths = {}
        for name in ('build_provenance', 'prepared_receipt'):
            relative = api.nonempty(record.get(name), label + ' ' + name)
            paths[name] = external(str(inventory.parent / relative), label + ' ' + name)
            api.require(paths[name].is_file(), label + ' missing ' + name)
        value = json.loads(paths['build_provenance'].read_text(encoding='utf-8'))
        validate_provenance(value, config, args.fixture, api, label)
        api.require(value['source_commit'] == plan['source_commit'] and value['version'] == plan['version'] and
            value['target'] == target and value['prepared_sha256'] == api.digest(paths['prepared_receipt']), label + ' source/prepared record mismatch')
        api.same_artifacts(value['artifacts'], api.artifact_bindings(plan, target), label)
        prepared = json.loads(paths['prepared_receipt'].read_text(encoding='utf-8'))
        api.require(prepared.get('version') == plan['version'] and prepared.get('target') == target and
            prepared.get('artifact') == Path(record['artifact']).name and prepared.get('signature') == record['signature'] and
            prepared.get('platform_signing') == record['platform_signing'], label + ' prepared receipt mismatch')
        identity = value['signing_identity']
        api.nonempty(identity, label + ' signing_identity')
        api.require(record['platform_signing'].get('identity' if args.fixture else 'certificate_sha1') == identity,
            label + ' native signing identity mismatch')
        if not args.fixture:
            api.require(re.fullmatch(r'[a-f0-9]{40}', identity), label + ' signing identity requires certificate SHA-1')
            if target == 'darwin-aarch64':
                api.require(type(record['platform_signing'].get('notarized')) is bool, 'Record macOS notarization honestly as true or false')
        manual_platform = 'windows-x86_64' if target.startswith('windows') else target
        manual = [item for item in release['installers'] if item['platform'] == manual_platform]
        api.require(prepared.get('installers') == [dict(item, path=Path(item['path']).name) for item in manual], label + ' prepared manual installer mismatch')
        for item in manual:
            api.require(item['platform_signing'].get('identity' if args.fixture else 'certificate_sha1') == identity,
                label + ' manual native signing identity mismatch')
    for obj in plan['objects']:
        external(obj['path'], 'artifact')
    return plan


@contextmanager
def publisher_lock(config, args, api):
    # OS-wide path on the designated publishing account: independent of checkout,
    # config/output directories, candidate route and process working directory.
    # Persistent files + OS advisory locks release automatically on process exit.
    if os.name == 'nt':
        root = Path(os.environ['LOCALAPPDATA']) / 'Token Market' / 'Release Publisher Locks'
    else:
        root = Path('/tmp') / ('tokenmarket-publisher-' + str(os.getuid()))
    external(str(root), 'publisher lock')
    root.mkdir(mode=0o700, parents=True, exist_ok=True)
    api.require(not root.is_symlink() and (os.name == 'nt' or root.stat().st_uid == os.getuid()), 'Unsafe publisher lock directory')
    destination = ('local-store:' + str(external(args.local_store, 'local_store'))) if args.fixture else (
        'oss://' + config['oss']['bucket'] + '/' + config['oss']['prefix'])
    name = hashlib.sha256(destination.encode()).hexdigest() + '.lock'
    path = root / name
    api.require(not path.is_symlink(), 'Unsafe publisher lock file')
    with path.open('a+b') as stream:
        try:
            if os.name == 'nt':
                import msvcrt
                if stream.tell() == 0: stream.write(b'0'); stream.flush()
                stream.seek(0)
                msvcrt.locking(stream.fileno(), msvcrt.LK_NBLCK, 1)
            else:
                import fcntl
                fcntl.flock(stream.fileno(), fcntl.LOCK_EX | fcntl.LOCK_NB)
        except OSError:
            raise ValueError('Another publisher owns this destination') from None
        try:
            yield
        finally:
            if os.name == 'nt':
                stream.seek(0)
                msvcrt.locking(stream.fileno(), msvcrt.LK_UNLCK, 1)
            else:
                fcntl.flock(stream.fileno(), fcntl.LOCK_UN)


def current_release_gate(config, args, api, plan):
    url = config['public_base_url'] + '/' + config['manifest_key']
    opener = api.build_opener(api.ProxyHandler({}) if args.fixture else api.ProxyHandler(), api.SafeRedirects(args.fixture))
    try:
        with opener.open(api.Request(url, headers={'Accept-Encoding': 'identity', 'Cache-Control': 'no-cache'}), timeout=30) as response:
            api.require(response.status == 200 and not response.headers.get('Content-Encoding') and
                response.headers.get_content_type() == 'application/json', 'Cannot validate current production manifest')
            cache = {part.strip().lower() for part in response.headers.get('Cache-Control', '').split(',')}
            api.require(all(part.strip().lower() in cache for part in api.DISCOVERY.split(',')), 'Cannot validate current production manifest cache policy')
            data = response.read(1024 * 1024 + 1)
            api.require(len(data) <= 1024 * 1024, 'Cannot validate current production manifest')
    except HTTPError as error:
        if error.code == 404:
            return  # Only a genuine public404 permits initial publication.
        raise ValueError('Cannot read current production manifest; publication stopped') from None
    except Exception:
        raise ValueError('Cannot read current production manifest; publication stopped') from None
    try:
        current = json.loads(data)
        version = current['version']
        api.require(isinstance(version, str) and re.fullmatch(r'(0|[1-9][0-9]*)[.](0|[1-9][0-9]*)[.](0|[1-9][0-9]*)', version), 'Invalid version')
        api.require(isinstance(current['notes'], str) and set(current['platforms']) == set(api.TARGETS), 'Invalid manifest')
        for entry in current['platforms'].values():
            api.public_base(entry['url'], args.fixture)
            api.nonempty(entry['signature'], 'Signature')
    except (ValueError, KeyError, TypeError):
        raise ValueError('Cannot validate current production manifest; publication stopped') from None
    previous, proposed = tuple(map(int, version.split('.'))), tuple(map(int, config['version'].split('.')))
    api.require(previous <= proposed, 'A newer production release is already published')
    if previous == proposed:
        proposed_manifest = plan['manifest']
        api.require(data == (json.dumps(proposed_manifest, indent=2) + '\n').encode(), 'Same-version production manifest differs; publish a newer version')


def checked_source(config, api):
    source = Path(config['source_root']).resolve()
    api.require(source.is_dir(), 'Missing source_root')
    sha = api.child(['git', '-C', source, 'rev-parse', 'HEAD'], label='Checked-out source inspection').strip()
    api.require(re.fullmatch(r'[a-f0-9]{40}', sha), 'source_root requires a full source commit')
    api.require(not api.child(['git', '-C', source, 'status', '--porcelain'], label='Clean source inspection').strip(),
        'Native local build requires a clean source_root including untracked files')
    actual = json.loads((source / 'src-tauri/tauri.conf.json').read_text(encoding='utf-8'))
    api.require(actual.get('version') == config['version'], 'version must match source_root desktop configuration')
    # Python3.9 lacks tomllib. This repository uses a literal package version;
    # accept that narrow static format and fail closed on inherited/dynamic TOML.
    manifest = (source / 'src-tauri/Cargo.toml').read_text(encoding='utf-8')
    section = re.search(r'(?ms)^\[package\][ \t]*\n(.*?)(?=^\[|\Z)', manifest)
    version = re.search(r'(?m)^version[ \t]*=[ \t]*"([0-9]+[.][0-9]+[.][0-9]+)"[ \t]*(?:#.*)?$', section.group(1)) if section else None
    api.require(version and version.group(1) == config['version'], 'src-tauri/Cargo.toml package.version must match version and tauri.conf.json')
    return source, sha

def native_build(args, config, api):
    source, sha = checked_source(config, api)
    api.require(args.fixture or source == Path(api.__file__).resolve().parents[1], 'source_root must be the checkout containing this release command')
    api.require(args.fixture or not args.fixture_native_command, '--fixture-native-command requires --fixture')
    target = args.target
    native = config['native']
    if not args.fixture:
        api.require(sys.platform == ('darwin' if target.startswith('darwin') else 'win32') and
            platform.machine().lower() in (('arm64', 'aarch64') if target.startswith('darwin') else ('amd64', 'x86_64')),
            'local-build requires the native host for target ' + target)
        names = ('macos_identity_dir', 'tauri_cli_js') if target.startswith('darwin') else ('windows_certificate_sha1', 'windows_timestamp_url', 'windows_signtool')
        missing = ['native.' + name for name in names if not native[name]]
        api.require(not missing, 'Missing local configuration: ' + ', '.join(missing))
        if target.startswith('darwin'):
            api.require(Path(native['macos_identity_dir']).is_dir(), 'Missing native.macos_identity_dir; initialize an identity separately only once')
            api.require(Path(native['tauri_cli_js']).is_file(), 'Missing native.tauri_cli_js')
        else:
            api.require(re.fullmatch(r'[a-fA-F0-9]{40}', native['windows_certificate_sha1']), 'Invalid native.windows_certificate_sha1')
            timestamp = api.urlsplit(native['windows_timestamp_url'])
            api.require(timestamp.scheme in ('http', 'https') and timestamp.hostname and not timestamp.username and not timestamp.password and not timestamp.fragment, 'Invalid native.windows_timestamp_url')
            api.require(Path(native['windows_signtool']).is_file(), 'Missing native.windows_signtool')
    else:
        api.require(args.fixture_native_command, 'Fixture native build requires --fixture-native-command')
    api.require(Path(config['private_key_file']).is_file(), 'Missing private_key_file')
    if config['private_key_password_file']:
        api.require(Path(config['private_key_password_file']).is_file(), 'Missing private_key_password_file')
    output = external(str(Path(config['output_dir']) / config['version'] / target), 'build output')
    api.require(not output.exists(), 'Native build output already exists; preserve finalized bytes')
    output.parent.mkdir(parents=True, exist_ok=True)
    started = api.datetime.now(api.timezone.utc).strftime('%Y-%m-%dT%H:%M:%SZ')
    with tempfile.TemporaryDirectory(prefix='.native-' + target + '-', dir=output.parent) as temporary:
        stage = Path(temporary)
        native_output = stage / 'native'
        env = {key: value for key, value in os.environ.items() if not key.startswith(
            ('OSS_', 'OSSUTIL_', 'GITHUB_', 'TAURI_', 'APPLE_', 'CSC_', 'TOKEN_MARKET_LOCAL_SIGNING_', 'CARGO_TARGET_DIR'))}
        env.update(TOKEN_MARKET_UPDATER_ENDPOINT=compiled(config)['endpoint'],
            TOKEN_MARKET_UPDATER_RECOVERY_URL=compiled(config)['recovery_url'], TOKEN_MARKET_UPDATER_PUBLIC_KEY=config['public_key'],
            CARGO_TARGET_DIR=str(stage / 'cargo-target'))
        if args.fixture:
            command = args.fixture_native_command + [str(native_output), target]
        elif target.startswith('darwin'):
            env.update(TOKEN_MARKET_LOCAL_SIGNING_DIR=native['macos_identity_dir'], TAURI_CLI_JS=native['tauri_cli_js'], TOKEN_MARKET_INDEPENDENT_CLI_VERSION='1')
            command = ['bash', source / 'scripts/build-macos-clients.sh', native_output]
        else:
            command = ['powershell.exe', '-NoProfile', '-NonInteractive', '-File', source / 'scripts/build-windows-clients.ps1',
                '-Output', native_output, '-CertificateSha1', native['windows_certificate_sha1'],
                '-TimestampUrl', native['windows_timestamp_url'], '-Signtool', native['windows_signtool'], '-Tauri', config['tools']['signer']]
        api.require(api.child([config['tools']['signer'], '--version'], label='Tauri CLI version check').strip() == 'tauri-cli 2.12.4', 'Tauri CLI 2.12.4 is required')
        api.require(Path(config['tools']['verifier']).is_file(), 'Missing tools.verifier')
        api.child(command, label='Native build/sign recipe', env=env, timeout=7200)
        api.require(checked_source(config, api)[1] == sha, 'Source changed during native build/sign recipe')
        if target.startswith('darwin'):
            apps = list(native_output.glob('*.app'))
            manuals = list(native_output.glob('*.dmg'))
            api.require(len(apps) == 1 and len(manuals) == 1, 'Native build requires exactly one final macOS app and DMG')
            app, installer = apps[0], None
        else:
            app, installer = native_output / 'TokenMarket.exe', native_output / 'setup.exe'
            manuals = [path for path in native_output.glob('manual.*') if path.suffix.lower() in ('.exe', '.msi')]
            api.require(len(manuals) == 1, 'Native build requires exactly one final Windows manual installer')
        signing = native_output / 'signing-manifest.json'
        api.require(signing.is_file(), 'Native build missing final signing-manifest.json')
        preparing = argparse.Namespace(version=config['version'], target=target, app=str(app),
            installer=str(installer) if installer else None, manual_installer=[str(path) for path in manuals],
            platform_evidence=str(signing), tauri_config=str(source / 'src-tauri/tauri.conf.json'),
            public_key=config['public_key_file'], output=str(stage / 'prepared'), signer=config['tools']['signer'],
            verifier=config['tools']['verifier'], fixture=args.fixture)
        # Private key exists only in the signer process environment; no key path,
        # private config, native child output or secrets are written into records.
        os.environ['TAURI_SIGNING_PRIVATE_KEY'] = Path(config['private_key_file']).read_text(encoding='utf-8').strip()
        os.environ['TAURI_SIGNING_PRIVATE_KEY_PASSWORD'] = Path(config['private_key_password_file']).read_text(encoding='utf-8').rstrip('\r\n') if config['private_key_password_file'] else ''
        try:
            with redirect_stdout(io.StringIO()):
                api.prepare(preparing)
        finally:
            os.environ.pop('TAURI_SIGNING_PRIVATE_KEY', None)
            os.environ.pop('TAURI_SIGNING_PRIVATE_KEY_PASSWORD', None)
        receipt = stage / 'prepared/prepared.json'
        prepared = json.loads(receipt.read_text(encoding='utf-8'))
        if args.fixture:
            # Synthetic classification never impersonates actual native verification.
            prepared['platform_signing']['identity'] = 'synthetic-native-adapter'
            for item in prepared['installers']:
                item['platform_signing']['identity'] = 'synthetic-native-adapter'
            receipt.write_text(json.dumps(prepared, indent=2) + '\n', encoding='utf-8')
        objects = []
        def artifact(name, role, artifact_target):
            path = receipt.parent / name
            objects.append(dict(role=role, platform=artifact_target, sha256=api.digest(path), size=path.stat().st_size))
        artifact(prepared['artifact'], 'updater', target)
        for item in prepared['installers']:
            artifact(item['path'], 'manual', item['platform'])
        identity = prepared['platform_signing'].get('identity' if args.fixture else 'certificate_sha1')
        api.nonempty(identity, 'Native signing identity')
        completed = api.datetime.now(api.timezone.utc).strftime('%Y-%m-%dT%H:%M:%SZ')
        record = dict(schema_version=2, fixture=args.fixture, provenance_kind='local-native-build', source_commit=sha,
            version=config['version'], target=target, artifacts=objects, prepared_sha256=api.digest(receipt),
            signing_identity=identity, compiled=compiled(config), configuration_sha256=policy_sha(config),
            native_host=socket.gethostname(), recipe='fixture-adapter' if args.fixture else Path(command[1] if target.startswith('darwin') else command[4]).name,
            started_at=started, completed_at=completed)
        validate_provenance(record, config, args.fixture, api, target)
        (receipt.parent / 'build.json').write_text(json.dumps(record, indent=2) + '\n', encoding='utf-8')
        api.require(checked_source(config, api)[1] == sha, 'Source changed during native preparation')
        # The large intermediate Cargo/native outputs are disposable; only prepared
        # exact updater/manual bytes and their two records survive atomically.
        os.rename(receipt.parent, output)
    print(json.dumps(dict(built=True, fixture=args.fixture, target=target, source_commit=sha,
        prepared_receipt=str(output / 'prepared.json'), build_provenance=str(output / 'build.json'))))

def collect_inventory(args, config, api):
    api.require(len(args.prepared) == len(api.TARGETS), 'Both required prepared target outputs are required')
    notes = external(args.notes_file, 'notes_file')
    api.require(notes.is_file(), 'Missing notes_file')
    api.nonempty(notes.read_text(encoding='utf-8'), 'Release notes')
    output = external(str(Path(config['output_dir']) / config['version'] / 'inventory'), 'inventory output')
    api.require(not output.exists(), 'Inventory output already exists; preserve finalized inventory')
    output.parent.mkdir(parents=True, exist_ok=True)
    with tempfile.TemporaryDirectory(prefix='.inventory-', dir=output.parent) as temporary:
        stage = Path(temporary)
        release = dict(version=config['version'], source_commit=None, public_key=config['public_key'],
            notes=notes.read_text(encoding='utf-8').strip(), targets={}, installers=[])
        for value in args.prepared:
            path = external(value, 'prepared')
            api.require(path.name == 'prepared.json' and path.is_file(), 'prepared requires a finalized prepared.json')
            prepared = json.loads(path.read_text(encoding='utf-8'))
            record_path = path.parent / 'build.json'
            api.require(record_path.is_file(), 'Prepared target missing local build.json')
            record = json.loads(record_path.read_text(encoding='utf-8'))
            validate_provenance(record, config, args.fixture, api, 'Prepared target')
            target = record['target']
            api.require(target in api.TARGETS and target not in release['targets'], 'Duplicate or unsupported prepared target')
            api.require(record['prepared_sha256'] == api.digest(path), 'Prepared target receipt digest mismatch')
            api.require(release['source_commit'] in (None, record['source_commit']), 'Prepared targets must share the same source commit')
            release['source_commit'] = record['source_commit']
            directory = stage / target
            directory.mkdir()
            names = [prepared['artifact'], 'prepared.json', 'build.json'] + [item['path'] for item in prepared['installers']]
            for name in names:
                api.require(isinstance(name, str) and Path(name).name == name and name not in ('', '.', '..'), 'Prepared target filenames must be basenames')
                source = external(str(path.parent / name), 'prepared artifact')
                api.require(source.is_file(), 'Missing prepared artifact')
                shutil.copyfile(source, directory / name)
            release['targets'][target] = dict(artifact=target + '/' + prepared['artifact'], signature=prepared['signature'],
                platform_signing=prepared['platform_signing'], build_provenance=target + '/build.json', prepared_receipt=target + '/prepared.json')
            release['installers'] += [dict(item, path=target + '/' + item['path']) for item in prepared['installers']]
        inventory = stage / 'release.json'
        inventory.write_text(json.dumps(release, indent=2) + '\n', encoding='utf-8')
        probe = argparse.Namespace(**vars(args))
        probe.inventory = str(inventory)
        plan = inventory_plan(probe, config, api)
        api.verify(plan, args.verifier)
        draft = json.loads((Path(api.__file__).resolve().parents[1] / 'docs/client-update-acceptance.template.json').read_text(encoding='utf-8'))
        draft['fixture'] = args.fixture
        draft['release'] = dict(source_commit=plan['source_commit'], version=plan['version'],
            artifacts=api.artifact_bindings(plan),
            manifest_sha256=hashlib.sha256((json.dumps(plan['manifest'], indent=2) + '\n').encode()).hexdigest())
        draft['routes'] = dict(public_base_url=config['public_base_url'],
            candidate_manifest_url=config['public_base_url'] + '/' + config['candidate_key'],
            production_manifest_url=compiled(config)['endpoint'], recovery_url=compiled(config)['recovery_url'],
            public_key_sha256=compiled(config)['public_key_sha256'])
        for target, record in release['targets'].items():
            device = draft['devices'][target]
            device.update(fixture=args.fixture, actual_device=False,
                build_provenance_sha256=api.digest(stage / record['build_provenance']),
                prepared_sha256=api.digest(stage / record['prepared_receipt']),
                signing_identity=json.loads((stage / record['build_provenance']).read_text(encoding='utf-8'))['signing_identity'],
                compiled=compiled(config))
            device['startup'].update(version=plan['version'], artifact_sha256=next(obj['sha256'] for obj in plan['objects'] if obj['role'] == 'updater' and obj['platform'] == target))
        (stage / 'acceptance-draft.json').write_text(json.dumps(draft, indent=2) + '\n', encoding='utf-8')
        os.rename(stage, output)
    print(json.dumps(dict(collected=True, fixture=args.fixture, inventory=str(output / 'release.json'), acceptance_template=str(output / 'acceptance-draft.json'))))

def run(args, api):
    config = load(args, api)
    args.local_configuration = config
    api.require(not args.local_store or args.fixture, '--local-store requires --fixture')
    if args.fixture and args.command in ('local-candidate', 'local-validate', 'local-publish'):
        api.require(args.local_store, '--fixture requires isolated --local-store; synthetic records cannot reach OSS')
        external(args.local_store, 'local_store')
    with environment(config):
        if args.command == 'local-build':
            native_build(args, config, api)
        elif args.command == 'local-inventory':
            collect_inventory(args, config, api)
        elif args.command == 'local-candidate':
            args.manifest_key = config['candidate_key']
            plan = inventory_plan(args, config, api)
            with publisher_lock(config, args, api):
                api.publish_verified(args, plan)
        elif args.command in ('local-validate', 'local-publish'):
            external(args.acceptance, 'acceptance')
            inventory_plan(args, config, api)
            args.dry_run = args.command == 'local-validate'
            if args.dry_run:
                api.promote(args)
            else:
                with publisher_lock(config, args, api):
                    plan = api.promotion_plan(args)
                    current_release_gate(config, args, api, plan)
                    api.promote(args, plan)
        else:
            raise ValueError('Local release input not yet prepared')
