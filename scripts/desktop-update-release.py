#!/usr/bin/env python3
"""Prepare, publish isolated candidates, and gate accepted desktop release promotion."""
from __future__ import annotations
import argparse
from datetime import datetime, timezone
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

# A release command must not dirty its own clean source checkout when importing
# the local orchestration module on standard CPython without a cache prefix.
sys.dont_write_bytecode = True

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


def inventory_plan(args, *, production_route=None):
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
    route = safe_key(production_route if production_route is not None else args.manifest_key)
    if production_route is None:
        require(route.startswith('test/') and route.endswith('/stable.json'), 'Only isolated test/<name>/stable.json routes are enabled; use the separately gated promote command for production')
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


def release_plan(args):
    # No command-line production bypass exists on plan/publish.
    return inventory_plan(args)


def child(command, *, stdin=None, label='Release tool', env=None, timeout=300):
    try:
        if env is None:
            env = {key: value for key, value in os.environ.items() if not key.startswith('TAURI_SIGNING_')}
        result = subprocess.run([str(part) for part in command], input=stdin, text=True,
                                capture_output=True, env=env, timeout=timeout)
    except (OSError, UnicodeError, subprocess.TimeoutExpired):
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
    require(args.fixture, 'Use local-candidate with private --config for real publication; legacy environment publishing is disabled')
    publish_verified(args, release_plan(args))


def publish_verified(args, plan):
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
        recovery_alias = dict(recovery_obj, key=plan['recovery_key'], cache_control=DISCOVERY) if 'recovery_key' in plan else None
        manifest = stage / 'stable.json'
        manifest.write_text(json.dumps(plan['manifest'], indent=2) + '\n')
        manifest_obj = dict(path=str(manifest), key=plan['manifest_key'], sha256=digest(manifest),
                            size=manifest.stat().st_size, content_type='application/json', cache_control=DISCOVERY)
        store = Path(local).resolve() if local else None
        if store:
            store.mkdir(parents=True, exist_ok=True)
        # Local production commands hold the canonical OS destination lock.
        # This extra fixture-store marker supports the legacy native fixture driver.
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
            if recovery_alias:
                put(recovery_alias, False)
                public_readback(plan, recovery_alias)
            # The only discovery write is last, after every public GET succeeds.
            put(manifest_obj, False)
            try:
                public_readback(plan, manifest_obj)
            except ValueError:
                raise ValueError('Final manifest PUT succeeded but public readback failed; discovery may have advanced; inspect discovery state before retrying') from None
        finally:
            lock.rmdir()
    print(json.dumps(dict(published=True, manifest_url=plan['manifest_url'],
                          recovery_url=plan['public_base_url'] + '/' + (recovery_alias or recovery_obj)['key'],
                          mode='synthetic-fixture' if args.fixture else 'production' if 'recovery_key' in plan else 'isolated-candidate',
                          actual_device_acceptance='operator-recorded; not independently authenticated' if 'recovery_key' in plan and not args.fixture else False)))


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


ACCEPTANCE_CHECKS = (
    'signed_out', 'zero_balance', 'automatic_discovery', 'manual_discovery',
    'release_notes', 'download_consent', 'install_consent', 'later_process_restart',
    'ignore_process_restart', 'ignored_manual_discovery', 'newer_version_reminder',
    'desktop_busy_refusal_retry', 'cli_mutation_refusal_retry', 'guard_transition_race',
    'active_work_not_cancelled', 'client_session_retained', 'credential_identity_retained',
    'tool_connections_retained', 'billing_owner_retained', 'models_retained',
    'user_configuration_retained', 'restoration_records_retained', 'independent_cli_use',
    'network_failure_retry', 'invalid_package_refusal_retry', 'installer_failure_retry',
    'external_recovery_opened', 'legacy_manual_bootstrap_then_update')
PROMOTION_CONFIG = ('TOKEN_MARKET_UPDATE_PRODUCTION_MANIFEST_KEY', 'TOKEN_MARKET_UPDATE_RECOVERY_KEY',
    'TOKEN_MARKET_UPDATE_PUBLIC_BASE_URL')


def nonempty(value, label):
    require(isinstance(value, str) and bool(value.strip()), label + ' must be a nonempty string')
    return value


def object_fields(value, fields, label):
    require(isinstance(value, dict) and set(value) == set(fields), label + ' requires exactly: ' + ', '.join(fields))


def evidence_passed(value, label, fixture, *, extra=(), local=False):
    object_fields(value, ('status', 'evidence') + tuple(extra), label)
    require(value['status'] == 'passed', label + ' must have status passed')
    refs = value['evidence']
    require(isinstance(refs, list) and bool(refs), label + ' requires concrete result/log references')
    for ref in refs:
        if local and isinstance(ref, dict):
            import desktop_update_local
            object_fields(ref, ('path', 'sha256'), label + ' local evidence')
            path = desktop_update_local.external(ref['path'], label + ' evidence.path')
            require(path.is_file() and digest(path) == ref['sha256'], label + ' local evidence digest mismatch')
            continue
        nonempty(ref, label + ' evidence reference')
        parsed = urlsplit(ref)
        require((fixture and parsed.scheme == 'synthetic') or
                (parsed.scheme == 'https' and parsed.hostname and not parsed.username and not parsed.password),
                label + ' requires HTTPS evidence references (synthetic requires --fixture)')
        if not fixture:
            require(not re.search(r'fixture|synthetic|example[.]|localhost|127[.]0[.]0[.]1', ref, re.I) and
                    not parsed.hostname.endswith(('.invalid', '.test', '.localhost')),
                    label + ' cannot use synthetic evidence')


def artifact_bindings(plan, target=None):
    platforms = (target, 'windows-x86_64') if target == 'windows-x86_64-nsis' else (target,)
    return [dict(role=obj['role'], platform=obj['platform'], sha256=obj['sha256'], size=obj['size'])
            for obj in plan['objects'] if target is None or obj['platform'] in platforms]


def same_artifacts(value, expected, label):
    require(isinstance(value, list) and len(value) == len(expected), label + ' artifact set mismatch')
    for item in value:
        object_fields(item, ('role', 'platform', 'sha256', 'size'), label + ' artifact')
        require(type(item['size']) is int and item['size'] > 0, label + ' artifact size must be a positive integer')
    ordering = lambda item: (item['platform'], item['role'], item['sha256'], item['size'])
    require(sorted(value, key=ordering) == sorted(expected, key=ordering), label + ' artifact digest/size/target mismatch')


def promotion_plan(args):
    local = getattr(args, 'local_configuration', None)
    require(args.fixture or local, 'Use local-publish with private --config (manifest_key/recovery_key); legacy CI/environment promotion is disabled')
    required = PROMOTION_CONFIG
    missing = [name for name in dict.fromkeys(required) if not os.environ.get(name, '').strip()]
    require(not missing, 'Missing configuration: ' + ', '.join(missing))
    require(not args.fixture or args.local_store, '--fixture requires an isolated --local-store; synthetic acceptance cannot reach OSS')
    require(not args.local_store or args.fixture, '--local-store requires --fixture')
    route = safe_key(os.environ['TOKEN_MARKET_UPDATE_PRODUCTION_MANIFEST_KEY'])
    recovery_key = safe_key(os.environ['TOKEN_MARKET_UPDATE_RECOVERY_KEY'])
    require(route.endswith('stable.json') and not route.startswith(('test/', 'releases/')), 'Production manifest must be an explicit stable route outside test/ and releases/')
    require(recovery_key.endswith('.html') and not recovery_key.startswith(('test/', 'releases/')) and recovery_key != route,
            'Recovery must be a fixed HTML alias outside test/ and immutable releases/')
    plan = inventory_plan(args, production_route=route)
    plan['recovery_key'] = recovery_key
    if not local:
        # Existing low-level fixture driver uses the same explicit local schema.
        local = dict(version=plan['version'], public_key=plan['public_key'], public_base_url=plan['public_base_url'],
            manifest_key=route, recovery_key=recovery_key)
    accepted = json.loads(Path(args.acceptance).read_text())
    object_fields(accepted, ('schema_version', 'fixture', 'release', 'routes', 'devices', 'evidence', 'provenance_kind'), 'Acceptance')
    require(type(accepted['schema_version']) is int and accepted['schema_version'] == 2, 'Unsupported acceptance schema_version')
    if local:
        require(accepted['provenance_kind'] == 'local-operator-acceptance', 'Invalid local acceptance provenance_kind')
    require(accepted['fixture'] is args.fixture, 'Acceptance fixture classification mismatch')
    binding = accepted['release']
    object_fields(binding, ('source_commit', 'version', 'artifacts', 'manifest_sha256'), 'Acceptance release')
    require(binding['source_commit'] == plan['source_commit'] and binding['version'] == plan['version'], 'Acceptance source/version mismatch')
    same_artifacts(binding['artifacts'], artifact_bindings(plan), 'Acceptance release')
    manifest_sha = hashlib.sha256((json.dumps(plan['manifest'], indent=2) + '\n').encode()).hexdigest()
    require(binding['manifest_sha256'] == manifest_sha, 'Acceptance candidate manifest bytes mismatch')
    routes = accepted['routes']
    object_fields(routes, ('public_base_url', 'candidate_manifest_url', 'production_manifest_url', 'recovery_url', 'public_key_sha256'), 'Acceptance routes')
    fingerprint = hashlib.sha256(plan['public_key'].strip().encode()).hexdigest()
    require(routes['public_base_url'] == plan['public_base_url'] and routes['production_manifest_url'] == plan['manifest_url'] and
            routes['recovery_url'] == plan['public_base_url'] + '/' + recovery_key and routes['public_key_sha256'] == fingerprint,
            'Acceptance route/key binding mismatch')
    candidate = nonempty(routes['candidate_manifest_url'], 'Candidate manifest URL')
    require(candidate.startswith(plan['public_base_url'] + '/test/') and candidate.endswith('/stable.json'),
            'Accepted candidate must use an isolated test route on the configured public service')
    candidate_key = candidate[len(plan['public_base_url']) + 1:]
    safe_key(candidate_key)
    expected_compiled = dict(endpoint=plan['manifest_url'], recovery_url=routes['recovery_url'], public_key_sha256=fingerprint)
    object_fields(accepted['devices'], TARGETS, 'Acceptance devices')
    object_fields(accepted['evidence'], ('automated', 'native_signing', 'deployment_readback'), 'Acceptance evidence')
    for name, evidence in accepted['evidence'].items():
        evidence_passed(evidence, name, args.fixture, local=bool(local))
    release = json.loads(Path(args.inventory).read_text())
    for target in TARGETS:
        label = 'Acceptance ' + target
        record = release['targets'][target]
        device = accepted['devices'][target]
        object_fields(device, ('actual_device', 'fixture', 'operator', 'tested_at', 'os_version', 'architecture',
            'starting_version', 'starting_updater_enabled', 'starting_install_path', 'installed_upgrade', 'startup',
            'build_provenance_sha256', 'prepared_sha256', 'signing_identity', 'compiled', 'checks'), label)
        require(device['fixture'] is args.fixture and device['actual_device'] is (not args.fixture), label + ' requires actual installed-client device evidence; fixtures never fill device slots')
        for field in ('operator', 'os_version', 'starting_install_path'):
            nonempty(device[field], label + ' ' + field)
        try:
            tested_at = datetime.strptime(device['tested_at'], '%Y-%m-%dT%H:%M:%SZ').replace(tzinfo=timezone.utc)
        except (ValueError, TypeError):
            raise ValueError(label + ' tested_at must be a UTC date/time') from None
        require(tested_at <= datetime.now(timezone.utc), label + ' tested_at cannot be in the future')
        require(device['architecture'] == ('arm64' if target == 'darwin-aarch64' else 'x86_64'), label + ' architecture mismatch')
        older = device['starting_version']
        require(isinstance(older, str) and re.fullmatch(r'(0|[1-9][0-9]*)[.](0|[1-9][0-9]*)[.](0|[1-9][0-9]*)', older) and
                tuple(map(int, older.split('.'))) < tuple(map(int, plan['version'].split('.'))) and device['starting_updater_enabled'] is True,
                label + ' requires an older updater-enabled installed starting version')
        evidence_passed(device['installed_upgrade'], label + ' installed upgrade', args.fixture, local=bool(local))
        evidence_passed(device['startup'], label + ' startup', args.fixture, extra=('version', 'executable', 'artifact_sha256'), local=bool(local))
        updater = next(obj for obj in plan['objects'] if obj['role'] == 'updater' and obj['platform'] == target)
        require(device['startup']['version'] == plan['version'] and device['startup']['artifact_sha256'] == updater['sha256'], label + ' new installed startup version/digest mismatch')
        nonempty(device['startup']['executable'], label + ' new installed executable')
        object_fields(device['checks'], ACCEPTANCE_CHECKS, label + ' checks')
        for name, evidence in device['checks'].items():
            evidence_passed(evidence, label + ' ' + name, args.fixture, local=bool(local))
        require(device['compiled'] == expected_compiled, label + ' compiled endpoint/recovery/key mismatch')
        path = Path(args.inventory).resolve().parent
        provenance_path = path / nonempty(record.get('build_provenance'), label + ' build_provenance path')
        prepared_path = path / nonempty(record.get('prepared_receipt'), label + ' prepared_receipt path')
        require(provenance_path.is_file() and prepared_path.is_file(), label + ' missing build provenance/prepared receipt')
        require(digest(provenance_path) == device['build_provenance_sha256'] and digest(prepared_path) == device['prepared_sha256'], label + ' build provenance/prepared receipt digest mismatch')
        provenance = json.loads(provenance_path.read_text())
        import desktop_update_local
        desktop_update_local.validate_provenance(provenance, local, args.fixture, sys.modules[__name__], label)
        require(provenance['source_commit'] == plan['source_commit'] and provenance['version'] == plan['version'] and
                provenance['target'] == target and provenance['compiled'] == expected_compiled and
                provenance['prepared_sha256'] == device['prepared_sha256'], label + ' source/build/compiled provenance mismatch')
        same_artifacts(provenance['artifacts'], artifact_bindings(plan, target), label + ' build provenance')
        identity = nonempty(device['signing_identity'], label + ' signing identity')
        require(provenance['signing_identity'] == identity and
                record['platform_signing'].get('identity' if args.fixture else 'certificate_sha1') == identity,
                label + ' native signing identity mismatch')
        if not args.fixture:
            require(re.fullmatch(r'[a-f0-9]{40}', identity), label + ' native identity must be a certificate SHA-1')
            if target == 'darwin-aarch64':
                require(type(record['platform_signing'].get('notarized')) is bool, 'Record macOS notarization honestly as true or false')
        prepared = json.loads(prepared_path.read_text())
        require(prepared.get('version') == plan['version'] and prepared.get('target') == target and
                prepared.get('artifact') == Path(record['artifact']).name and prepared.get('signature') == record['signature'] and
                prepared.get('platform_signing') == record['platform_signing'], label + ' prepared receipt does not match finalized inventory')
        manual = [item for item in release['installers'] if item['platform'] == ('windows-x86_64' if target.startswith('windows') else target)]
        normalized_manual = [dict(item, path=Path(item['path']).name) for item in manual]
        require(prepared.get('installers') == normalized_manual, label + ' prepared manual installer evidence mismatch')
        for item in manual:
            require(item['platform_signing'].get('identity' if args.fixture else 'certificate_sha1') == identity,
                    label + ' manual installer native signing identity mismatch')
    return plan


def promote(args, plan=None):
    if plan is None:
        plan = promotion_plan(args)
    if args.dry_run:
        # Verify the same snapshots without credentials, network, locks or writes.
        with tempfile.TemporaryDirectory(prefix='tokenmarket-promotion-plan-') as temporary:
            for index, obj in enumerate(plan['objects']):
                snapshot = Path(temporary) / str(index)
                shutil.copyfile(obj['path'], snapshot)
                require(digest(snapshot) == obj['sha256'], 'Artifact changed after acceptance validation')
                obj['path'] = str(snapshot)
            verify(plan, args.verifier)
        print(json.dumps(dict(validated=True, dry_run=True, mode='synthetic-fixture' if args.fixture else 'production',
                              manifest_url=plan['manifest_url'], recovery_url=plan['public_base_url'] + '/' + plan['recovery_key'],
                              actual_device_acceptance=False if args.fixture else 'operator-recorded; not independently authenticated')))
    else:
        publish_verified(args, plan)


def main():
    os.umask(0o077)
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
    promotion = commands.add_parser('promote', help='Validate installed-client acceptance and promote; --dry-run performs no writes')
    promotion.add_argument('--inventory', required=True)
    promotion.add_argument('--acceptance', required=True)
    promotion.add_argument('--dry-run', action='store_true')
    promotion.add_argument('--fixture', action='store_true')
    promotion.add_argument('--local-store')
    promotion.add_argument('--verifier')
    promotion.add_argument('--ossutil', default='ossutil')
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
    for name in ('local-build', 'local-inventory', 'local-candidate', 'local-validate', 'local-publish'):
        command = commands.add_parser(name, help='Explicit local operator release workflow; private JSON config outside Git')
        command.add_argument('--config', required=True)
        command.add_argument('--fixture', action='store_true')
        command.add_argument('--local-store')
        if name == 'local-build':
            command.add_argument('--target', choices=TARGETS, required=True)
            command.add_argument('--fixture-native-command', nargs='+')
        elif name == 'local-inventory':
            command.add_argument('--prepared', action='append', required=True)
            command.add_argument('--notes-file', required=True)
        else:
            command.add_argument('--inventory', required=True)
            if name != 'local-candidate': command.add_argument('--acceptance', required=True)
    args = parser.parse_args()
    if args.command.startswith('local-'):
        import desktop_update_local
        desktop_update_local.run(args, sys.modules[__name__])
    elif args.command == 'plan':
        print(json.dumps(release_plan(args), indent=2))
    elif args.command == 'publish':
        publish(args)
    elif args.command == 'promote':
        promote(args)
    elif args.command == 'prepare':
        prepare(args)


if __name__ == '__main__':
    try:
        main()
    except (ValueError, OSError, KeyError, TypeError, AttributeError) as error:
        print('Release rejected: ' + (str(error) if isinstance(error, ValueError) and not isinstance(error, json.JSONDecodeError) else 'Invalid or unavailable release input'), file=sys.stderr)
        sys.exit(1)
