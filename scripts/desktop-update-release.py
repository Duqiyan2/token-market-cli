#!/usr/bin/env python3
"""Prepare and publish signed desktop releases. Production promotion is disabled."""
from __future__ import annotations
import argparse
import hashlib
import html
import json
import os
from pathlib import Path
import re
import shutil
import subprocess
import tempfile
import tarfile
import plistlib
import struct
from urllib.request import build_opener, HTTPRedirectHandler, ProxyHandler, Request
import sys
from urllib.parse import quote, unquote, urlsplit

TARGETS = ('darwin-aarch64', 'windows-x86_64-nsis')
CONFIG = ('OSS_REGION', 'OSS_ENDPOINT', 'TOKEN_MARKET_UPDATE_OSS_BUCKET',
          'TOKEN_MARKET_UPDATE_OSS_PREFIX', 'TOKEN_MARKET_UPDATE_PUBLIC_BASE_URL')
IMMUTABLE = 'public, max-age=31536000, immutable'
DISCOVERY = 'no-cache, no-store, max-age=0, must-revalidate'


def require(condition, message):
    if not condition:
        raise ValueError(message)


def digest(path):
    value = hashlib.sha256()
    with path.open('rb') as stream:
        for chunk in iter(lambda: stream.read(1024 * 1024), b''):
            value.update(chunk)
    return value.hexdigest()


def safe_key(key):
    require(isinstance(key, str) and key and not key.startswith('/') and
            all(part not in ('', '.', '..') for part in key.split('/')) and
            re.fullmatch(r'[A-Za-z0-9._/-]+', key), 'Invalid object key')
    return key


def public_base(value, fixture):
    parsed = urlsplit(value)
    loopback = fixture and parsed.scheme == 'http' and parsed.hostname in ('127.0.0.1', 'localhost', '::1')
    require((parsed.scheme == 'https' or loopback) and parsed.hostname and
            not parsed.username and not parsed.password and not parsed.query and not parsed.fragment,
            'Public base URL must use HTTPS (loopback HTTP requires --fixture)')
    require(not any(part in ('.', '..') for part in unquote(parsed.path).split('/')), 'Invalid public base path')
    return value.rstrip('/')


def release_plan(args):
    inventory = Path(args.inventory).resolve()
    release = json.loads(inventory.read_text())
    version = release.get('version', '')
    require(re.fullmatch(r'(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)', version), 'A stable semantic version is required')
    require(re.fullmatch(r'[a-f0-9]{40}', release.get('source_commit', '')), 'source_commit must be a full commit SHA')
    require(isinstance(release.get('notes'), str) and release['notes'].strip(), 'Release notes are required')
    require(isinstance(release.get('public_key'), str) and release['public_key'].strip(), 'Updater public_key is required')
    require(set(release.get('targets', {})) == set(TARGETS), 'Both required updater targets are required: ' + ', '.join(TARGETS))
    base = os.environ.get('TOKEN_MARKET_UPDATE_PUBLIC_BASE_URL', '').strip()
    base = public_base(base, args.fixture) if base else None
    prefix = os.environ.get('TOKEN_MARKET_UPDATE_OSS_PREFIX', '').strip()
    if prefix:
        safe_key(prefix)
    # This command deliberately cannot publish production discovery before #38.
    route = safe_key(args.manifest_key)
    require(route.startswith('test/') and route.endswith('/stable.json'), 'Only isolated test/<name>/stable.json routes are enabled; production promotion remains disabled')
    objects = []

    def artifact(relative, role, platform):
        require(isinstance(relative, str) and relative, 'Artifact path is required')
        source = (inventory.parent / relative).resolve()
        require(source.is_file() and not source.is_symlink(), 'Artifact file is missing')
        require(source.stat().st_size > 0, 'Artifact is empty')
        name = re.sub(r'[^A-Za-z0-9._-]', '-', source.name)
        sha = digest(source)
        key = f'releases/{version}/{sha}/{name}'
        content_type = 'application/gzip' if name.endswith('.tar.gz') else 'application/octet-stream'
        obj = dict(path=str(source), key=key, sha256=sha, size=source.stat().st_size,
                   role=role, platform=platform, content_type=content_type, cache_control=IMMUTABLE)
        objects.append(obj)
        return obj

    platforms = {}
    for target in TARGETS:
        record = release['targets'][target]
        evidence = record.get('platform_signing', {})
        if not args.fixture:
            require(not evidence.get('fixture') and evidence.get('verified') is True and evidence.get('kind') == ('codesign' if target == 'darwin-aarch64' else 'authenticode'), 'Final platform signing evidence is required')
        require(isinstance(record.get('signature'), str) and record['signature'].strip(), 'Updater signature is required')
        suffix = '.app.tar.gz' if target == 'darwin-aarch64' else '.exe'
        require(record.get('artifact', '').endswith(suffix), 'Incorrect updater artifact format for ' + target)
        obj = artifact(record['artifact'], 'updater', target)
        if not args.fixture:
            require(evidence.get('artifact_sha256') == obj['sha256'], 'Platform signing evidence does not match finalized artifact bytes')
            if target == 'windows-x86_64-nsis':
                require(evidence.get('bundler_version') == '2.10.4' and evidence.get('nsis_install_mode') == 'currentUser', 'Only pinned currentUser NSIS artifacts are supported for automatic updates')
        obj['signature'] = record['signature'].strip()
        platforms[target] = dict(url=f'{base}/{quote(obj["key"])}' if base else None, signature=obj['signature'])
    installers = release.get('installers', [])
    require({item.get('platform') for item in installers} >= {'darwin-aarch64', 'windows-x86_64'}, 'Manual installers for both platforms are required')
    for item in installers:
        require(item.get('platform') in ('darwin-aarch64', 'windows-x86_64'), 'Unsupported manual installer platform')
        require(item.get('path', '').lower().endswith(('.dmg',) if item['platform'] == 'darwin-aarch64' else ('.exe', '.msi')), 'Incorrect manual installer format')
        obj = artifact(item.get('path'), 'manual', item.get('platform'))
        if not args.fixture:
            evidence = item.get('platform_signing', {})
            require(not evidence.get('fixture') and evidence.get('verified') is True and
                    evidence.get('kind') == ('codesign' if item['platform'] == 'darwin-aarch64' else 'authenticode') and
                    evidence.get('artifact_sha256') == obj['sha256'], 'Final manual installer signing evidence is required')
    manifest = dict(version=version, notes=release['notes'], platforms=platforms)
    missing = [field for field in CONFIG if not os.environ.get(field, '').strip()]
    return dict(version=version, source_commit=release['source_commit'], public_key=release['public_key'],
                fixture=args.fixture, objects=objects, manifest=manifest, manifest_key=route,
                manifest_url=f'{base}/{quote(route)}' if base else None, missing_configuration=missing,
                public_base_url=base, prefix=prefix or None)


def child(command, *, stdin=None, label='Release tool', env=None):
    try:
        if env is None:
            env = {key: value for key, value in os.environ.items() if not key.startswith('TAURI_SIGNING_')}
        result = subprocess.run([str(part) for part in command], input=stdin, text=True,
                                capture_output=True, env=env, timeout=300)
    except (OSError, subprocess.TimeoutExpired):
        raise ValueError(label + ' failed; child output suppressed') from None
    require(result.returncode == 0, label + ' failed; child output suppressed')
    return result.stdout


def verify(plan, executable):
    require(executable and Path(executable).is_file(), 'Build and supply --verifier (official updater 2.14.1 artifact verifier)')
    clean = {key: value for key, value in os.environ.items() if not key.startswith(('TAURI_SIGNING_', 'OSS_', 'OSSUTIL_'))}
    output = child([Path(executable).resolve()], stdin=json.dumps(plan), label='Updater signature/version verifier', env=clean)
    verified = json.loads(output)
    require(verified.get('verified') is True and verified.get('verifier') == 'tauri-plugin-updater/2.14.1', 'Updater verification did not succeed')


class SafeRedirects(HTTPRedirectHandler):
    def __init__(self, fixture):
        self.fixture = fixture
    def redirect_request(self, request, fp, code, msg, headers, newurl):
        public_base(newurl, self.fixture)
        return super().redirect_request(request, fp, code, msg, headers, newurl)


def public_readback(plan, obj):
    url = plan['public_base_url'] + '/' + quote(obj['key'])
    opener = build_opener(ProxyHandler({}) if plan['fixture'] else ProxyHandler(), SafeRedirects(plan['fixture']))
    try:
        with opener.open(Request(url, headers={'Accept-Encoding': 'identity'}), timeout=30) as response:
            require(response.status == 200, 'Public readback did not return 200')
            require(not response.headers.get('Content-Encoding'), 'Public route must not transform signed bytes')
            require(response.headers.get_content_type() == obj['content_type'], 'Public content type mismatch')
            cache = {part.strip().lower() for part in response.headers.get('Cache-Control', '').split(',')}
            require(all(part.strip().lower() in cache for part in obj['cache_control'].split(',')), 'Public cache policy mismatch')
            value, size = hashlib.sha256(), 0
            for chunk in iter(lambda: response.read(1024 * 1024), b''):
                value.update(chunk)
                size += len(chunk)
                require(size <= obj['size'], 'Public readback exceeds expected size')
            require(size == obj['size'] and value.hexdigest() == obj['sha256'], 'Public readback digest/size mismatch')
    except ValueError:
        raise
    except Exception:
        raise ValueError('Public readback failed; inspect discovery state before retrying') from None


def recovery_document(plan):
    links = ''.join('<li><a href="' + html.escape(plan['public_base_url'] + '/' + quote(obj['key']), quote=True) + '">' +
                    html.escape(obj['platform'] + ': ' + Path(obj['path']).name) + '</a></li>'
                    for obj in plan['objects'] if obj['role'] == 'manual')
    return ('<!doctype html><html lang="en"><meta charset="utf-8"><title>Token Market recovery</title>'
            '<h1>Token Market ' + html.escape(plan['version']) + '</h1><ul>' + links + '</ul>'
            '<p>Clients without an updater require one manual installation of an updater-enabled release.</p>'
            '<p>Retry a failed download first. If the application cannot start, quit it and install the same or a newer supported official release over the existing installation.</p>'
            '<p>Keep shared client data, Client Session, Tool Connections, credentials, configuration and restoration backups. Do not delete the Token Market data directory or clear system credentials.</p>'
            '<p>There is no automatic rollback. Arbitrary older releases may not read data written by newer versions; do not downgrade without explicit compatibility guidance.</p>'
            '<p>Windows MSI installers are for manual recovery; in-app installation initially supports eligible currentUser NSIS installations only.</p></html>').encode()


def publish(args):
    plan = release_plan(args)
    local = args.local_store
    require(not local or args.fixture, '--local-store requires --fixture; fixture signing evidence cannot reach OSS')
    require(not args.fixture or local, '--fixture requires an isolated --local-store')
    required = ['TOKEN_MARKET_UPDATE_PUBLIC_BASE_URL'] if local else plan['missing_configuration']
    missing = [name for name in required if not os.environ.get(name, '').strip()]
    require(not missing, 'Missing configuration: ' + ', '.join(missing))
    if not local:
        explicit_sts = all(os.environ.get(name) for name in ('OSS_ACCESS_KEY_ID', 'OSS_ACCESS_KEY_SECRET', 'OSS_SESSION_TOKEN'))
        explicit_profile = os.environ.get('OSSUTIL_CONFIG_FILE') and os.environ.get('OSSUTIL_PROFILE')
        require(explicit_sts or explicit_profile, 'Missing upload identity: provide OSS STS env triplet or explicit OSSUTIL_CONFIG_FILE and OSSUTIL_PROFILE')
        version = child([args.ossutil, 'version'], label='ossutil version check')
        require(re.search(r'(?<![0-9])2\.4\.1(?![0-9])', version), 'ossutil 2.4.1 is required')
        help_text = child([args.ossutil, 'cp', '--help'], label='ossutil upload interface check')
        require(all(flag in help_text for flag in ('--cache-control', '--content-type', '--ignore-existing', '--metadata')), 'Unsupported ossutil 2.x upload interface')
    # Snapshot every artifact first: the verified bytes and uploaded bytes must match.
    with tempfile.TemporaryDirectory(prefix='tokenmarket-release-') as temporary:
        stage = Path(temporary)
        for index, obj in enumerate(plan['objects']):
            source = stage / str(index) / Path(obj['path']).name
            source.parent.mkdir()
            shutil.copyfile(obj['path'], source)
            require(digest(source) == obj['sha256'], 'Artifact changed after inventory validation')
            obj['path'] = str(source)
        verify(plan, args.verifier)
        recovery = stage / 'recovery.html'
        recovery.write_bytes(recovery_document(plan))
        recovery_sha = digest(recovery)
        recovery_obj = dict(path=str(recovery), key=f'releases/{plan["version"]}/{recovery_sha}/recovery.html',
                            sha256=recovery_sha, size=recovery.stat().st_size, content_type='text/html', cache_control=IMMUTABLE)
        objects = plan['objects'] + [recovery_obj]
        manifest = stage / 'stable.json'
        manifest.write_text(json.dumps(plan['manifest'], indent=2) + '\n')
        manifest_obj = dict(path=str(manifest), key=plan['manifest_key'], sha256=digest(manifest),
                            size=manifest.stat().st_size, content_type='application/json', cache_control=DISCOVERY)
        store = Path(local).resolve() if local else None
        if store:
            store.mkdir(parents=True, exist_ok=True)
        # One local publisher per destination. OSS release jobs must additionally
        # use a CI concurrency group spanning every publisher (see deployment doc).
        lock = (store / '.publish-lock') if store else stage / '.publish-lock'
        try:
            lock.mkdir()
        except FileExistsError:
            raise ValueError('Another publisher owns this destination') from None
        try:
            def put(obj, immutable):
                if store:
                    destination = store / obj['key']
                    destination.parent.mkdir(parents=True, exist_ok=True)
                    if immutable and destination.exists():
                        require(digest(destination) == obj['sha256'], 'Immutable object conflict')
                        return
                    pending = destination.with_name(destination.name + '.pending')
                    shutil.copyfile(obj['path'], pending)
                    os.replace(pending, destination)
                else:
                    key = plan['prefix'] + '/' + obj['key']
                    command = [args.ossutil, 'cp', obj['path'], 'oss://' + os.environ['TOKEN_MARKET_UPDATE_OSS_BUCKET'] + '/' + key,
                               '--cache-control', obj['cache_control'], '--content-type', obj['content_type'],
                               '--metadata', 'sha256=' + obj['sha256'], '--loglevel', 'off', '--quiet',
                               '--ignore-existing' if immutable else '--force']
                    if os.environ.get('TOKEN_MARKET_UPDATE_OSS_ADDRESSING_STYLE'):
                        command += ['--addressing-style', os.environ['TOKEN_MARKET_UPDATE_OSS_ADDRESSING_STYLE']]
                    child(command, label='OSS object upload')
            for obj in objects:
                put(obj, True)
                public_readback(plan, obj)
            # The only discovery write is last, after every public GET succeeds.
            put(manifest_obj, False)
            public_readback(plan, manifest_obj)
        finally:
            lock.rmdir()
    print(json.dumps(dict(published=True, manifest_url=plan['manifest_url'],
                          recovery_url=plan['public_base_url'] + '/' + recovery_obj['key'])))


def platform_signing(args, app, artifact, temporary):
    if args.fixture:
        return {'fixture': True}
    require(args.platform_evidence, 'Final platform signing evidence is required')
    expected = json.loads(Path(args.platform_evidence).read_text())
    if args.target == 'darwin-aarch64':
        require(sys.platform == 'darwin', 'macOS preparation must run on macOS')
        require(expected.get('profile') == 'production' and
                re.fullmatch(r'[A-Fa-f0-9]{40}', expected.get('certificateSha1', '')),
                'Production macOS signing manifest/certificateSha1 is required')
        child(['/usr/bin/codesign', '--verify', '--deep', '--strict', app], label='Final macOS application signature verification')
        cert = Path(temporary) / 'certificate'
        child(['/usr/bin/codesign', '-d', '--extract-certificates', cert, app], label='macOS signing identity extraction')
        certificate = cert.with_name(cert.name + '0')
        actual = hashlib.sha1(certificate.read_bytes()).hexdigest()
        require(actual.lower() == expected['certificateSha1'].lower(), 'macOS signing identity mismatch')
        info = plistlib.loads((app / 'Contents/Info.plist').read_bytes())
        require(info.get('CFBundleIdentifier') == 'cn.tokenmarket.setup' and
                info.get('CFBundleShortVersionString') == args.version, 'macOS bundle identity/version mismatch')
        binary = app / 'Contents/MacOS' / info['CFBundleExecutable']
        archs = child(['/usr/bin/lipo', '-archs', binary], label='macOS architecture verification').split()
        require(archs == ['arm64'], 'macOS updater requires Apple Silicon arm64')
        return dict(verified=True, kind='codesign', certificate_sha1=actual, notarized=expected.get('notarized') is True)
    require(sys.platform == 'win32', 'Windows preparation must run on Windows')
    if args.target == 'windows-x86_64-nsis':
        config = json.loads(Path(args.tauri_config).read_text())
        require(config.get('bundle', {}).get('windows', {}).get('nsis', {}).get('installMode') == 'currentUser', 'Automatic Windows packages require explicit currentUser NSIS build configuration')
    thumbprint = expected.get('certificateSha1', '')
    require(re.fullmatch(r'[A-Fa-f0-9]{40}', thumbprint), 'Windows signing certificateSha1 is required')
    # LiteralPath and separate argv keep paths out of PowerShell program text.
    script = Path(temporary) / 'verify-authenticode.ps1'
    script.write_text("""param([string]$Binary, [string]$Expected, [string]$Version)
$ErrorActionPreference = 'Stop'
$s = Get-AuthenticodeSignature -LiteralPath $Binary
if ($s.Status -ne 'Valid' -or $s.SignerCertificate.Thumbprint -ne $Expected) { exit 1 }
if ($Version -and [System.Diagnostics.FileVersionInfo]::GetVersionInfo($Binary).ProductVersion -ne $Version) { exit 2 }
""")
    for path, version in ((app, args.version), (artifact, '')):
        child(['powershell.exe', '-NoProfile', '-NonInteractive', '-File', script,
               '-Binary', path, '-Expected', thumbprint, '-Version', version], label='Final Windows Authenticode verification')
    with app.open('rb') as stream:
        header = stream.read(64)
        require(header[:2] == b'MZ', 'Windows app must be PE x64')
        stream.seek(struct.unpack_from('<I', header, 0x3c)[0])
        pe = stream.read(6)
    require(pe[:4] == b'PE\0\0' and pe[4:] == b'\x64\x86', 'Windows updater requires x64')
    return dict(verified=True, kind='authenticode', certificate_sha1=thumbprint.lower(),
                bundler_version='2.10.4', nsis_install_mode='currentUser' if args.target.endswith('-nsis') else None)


def prepare(args):
    require(re.fullmatch(r'(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)', args.version), 'A stable semantic version is required')
    require(os.environ.get('TAURI_SIGNING_PRIVATE_KEY', '').strip(), 'Missing TAURI_SIGNING_PRIVATE_KEY; signing keys are accepted through environment only')
    require(not os.environ.get('TAURI_SIGNING_PRIVATE_KEY_PATH'), 'Private key file-path fallback is disabled; use TAURI_SIGNING_PRIVATE_KEY env')
    output = Path(args.output).resolve()
    require(not output.exists(), 'Prepare output must not exist; existing artifacts were preserved')
    app = Path(args.app).resolve()
    key = Path(args.public_key).read_text().strip()
    require(key, 'Updater public key is required')
    require(args.target != 'darwin-aarch64' or (app.is_dir() and app.name.endswith('.app')), 'Final macOS .app is required')
    require(args.target == 'darwin-aarch64' or (app.is_file() and args.installer), 'Final Windows app and installer are required')
    version = child([args.signer, '--version'], label='Tauri CLI version check')
    require(version.strip() == 'tauri-cli 2.12.4', 'Tauri CLI 2.12.4 (bundler 2.10.4) is required')
    output.parent.mkdir(parents=True, exist_ok=True)
    with tempfile.TemporaryDirectory(prefix='tokenmarket-prepare-', dir=output.parent) as temporary:
        stage = Path(temporary)
        if args.target == 'darwin-aarch64':
            copied_app = stage / app.name
            shutil.copytree(app, copied_app, symlinks=True)
            evidence = platform_signing(args, copied_app, None, stage)
            artifact = stage / 'TokenMarket.app.tar.gz'
            with tarfile.open(artifact, 'w:gz') as archive:
                archive.add(copied_app, arcname=app.name)
            shutil.rmtree(copied_app)
        else:
            installer = Path(args.installer).resolve()
            suffix = '.msi' if args.target.endswith('-msi') else '.exe'
            require(installer.is_file() and installer.suffix.lower() == suffix, 'Final Windows installer format mismatch')
            artifact = stage / ('TokenMarket-' + args.version + suffix)
            shutil.copyfile(installer, artifact)
            evidence = platform_signing(args, app, artifact, stage)
        evidence['artifact_sha256'] = digest(artifact)
        manual_records = []
        for source in args.manual_installer:
            source = Path(source).resolve()
            destination = stage / source.name
            require(destination != artifact and not destination.exists(), 'Manual installer filename conflicts with prepared output')
            shutil.copyfile(source, destination)
            if args.target == 'darwin-aarch64':
                require(source.suffix.lower() == '.dmg', 'macOS manual installer must be a DMG')
                if not args.fixture:
                    prior = json.loads(Path(args.platform_evidence).read_text())
                    require(prior.get('artifacts', {}).get(source.name) == digest(destination), 'DMG does not match final macOS signing manifest')
                    child(['/usr/bin/hdiutil', 'verify', destination], label='Final manual DMG verification')
                manual_evidence = dict(evidence, artifact_sha256=digest(destination))
            else:
                require(source.suffix.lower() in ('.exe', '.msi'), 'Windows manual installer must be EXE or MSI')
                manual_evidence = platform_signing(args, app, destination, stage)
                manual_evidence['artifact_sha256'] = digest(destination)
            manual_records.append(dict(path=destination.name, platform='darwin-aarch64' if args.target == 'darwin-aarch64' else 'windows-x86_64', platform_signing=manual_evidence))
        signer_env = dict(os.environ)
        # Prevent CLI prompts; an empty password is still an explicit env-only value.
        signer_env.setdefault('TAURI_SIGNING_PRIVATE_KEY_PASSWORD', '')
        child([args.signer, 'signer', 'sign', '--app-version', args.version, artifact],
              label='Updater signing', env=signer_env)
        signature = artifact.with_name(artifact.name + '.sig').read_text().strip()
        record = dict(artifact=artifact.name, signature=signature, platform_signing=evidence)
        probe = dict(version=args.version, public_key=key, required_targets=1,
                     objects=[dict(path=str(artifact), platform=args.target, signature=signature, role='updater')])
        verify(probe, args.verifier)
        # The receipt contains public metadata only. Never persist signer env/output.
        (stage / 'prepared.json').write_text(json.dumps(dict(version=args.version, target=args.target, installers=manual_records, **record), indent=2) + '\n')
        for extra in list(stage.iterdir()):
            if extra not in (artifact, artifact.with_name(artifact.name + '.sig'), stage / 'prepared.json') and extra.name not in [item['path'] for item in manual_records]:
                if extra.is_dir():
                    shutil.rmtree(extra)
                else:
                    extra.unlink()
        # os.rename is atomic on this same filesystem; no partially prepared output.
        os.rename(stage, output)
    print(json.dumps(dict(prepared=True, receipt=str(output / 'prepared.json'))))


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    commands = parser.add_subparsers(dest='command', required=True)
    plan = commands.add_parser('plan', help='Read-only inventory; no credentials or network required')
    plan.add_argument('--inventory', required=True)
    plan.add_argument('--fixture', action='store_true')
    plan.add_argument('--manifest-key', default='test/candidate/stable.json')
    publishing = commands.add_parser('publish', help='Verify and publish an isolated candidate; production promotion disabled')
    publishing.add_argument('--inventory', required=True)
    publishing.add_argument('--fixture', action='store_true')
    publishing.add_argument('--manifest-key', default='test/candidate/stable.json')
    publishing.add_argument('--local-store')
    publishing.add_argument('--verifier', required=True)
    publishing.add_argument('--ossutil', default='ossutil')
    preparing = commands.add_parser('prepare', help='Package final OS-signed bytes and sign their exact updater artifact')
    preparing.add_argument('--version', required=True)
    preparing.add_argument('--target', choices=TARGETS + ('windows-x86_64-msi',), required=True)
    preparing.add_argument('--app', required=True)
    preparing.add_argument('--installer')
    preparing.add_argument('--manual-installer', action='append', default=[])
    preparing.add_argument('--platform-evidence')
    preparing.add_argument('--tauri-config', default=str(Path(__file__).resolve().parents[1] / 'src-tauri/tauri.conf.json'))
    preparing.add_argument('--public-key', required=True)
    preparing.add_argument('--output', required=True)
    preparing.add_argument('--signer', default='tauri')
    preparing.add_argument('--verifier')
    preparing.add_argument('--fixture', action='store_true')
    args = parser.parse_args()
    if args.command == 'plan':
        print(json.dumps(release_plan(args), indent=2))
    elif args.command == 'publish':
        publish(args)
    elif args.command == 'prepare':
        prepare(args)


if __name__ == '__main__':
    try:
        main()
    except (ValueError, OSError, KeyError, TypeError, AttributeError) as error:
        print('Release rejected: ' + (str(error) if isinstance(error, ValueError) and not isinstance(error, json.JSONDecodeError) else 'Invalid or unavailable release input'), file=sys.stderr)
        sys.exit(1)
