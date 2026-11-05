"""Release command contract; all objects live in an isolated fixture directory."""
from __future__ import annotations
import json
import shutil
import hashlib
import tarfile
import contextlib
from http.server import SimpleHTTPRequestHandler, ThreadingHTTPServer
import threading
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[1]
COMMAND = [sys.executable, str(ROOT / 'scripts/desktop-update-release.py')]
FIXTURES = ROOT / 'src-tauri/tests/fixtures/client-update'

class DistributionCommand(unittest.TestCase):
    def setUp(self):
        self.directory = tempfile.TemporaryDirectory()
        self.addCleanup(self.directory.cleanup)
        self.root = Path(self.directory.name)
        for name in ['mac.app.tar.gz', 'setup.exe', 'manual.dmg', 'manual.msi']:
            (self.root / name).write_bytes((FIXTURES / 'package.txt').read_bytes())
        signature = (FIXTURES / 'package.txt.sig').read_text().strip()
        self.inventory = self.root / 'release.json'
        self.release = dict(version='1.1.0', source_commit='a' * 40, notes='Release fixture',
            public_key=(FIXTURES / 'public-key.txt').read_text().strip(),
            targets={
                'darwin-aarch64': dict(artifact='mac.app.tar.gz', signature=signature, platform_signing={'fixture': True}),
                'windows-x86_64-nsis': dict(artifact='setup.exe', signature=signature, platform_signing={'fixture': True})},
            installers=[dict(path='manual.dmg', platform='darwin-aarch64'),
                        dict(path='manual.msi', platform='windows-x86_64')])
        self.save()

    def save(self):
        self.inventory.write_text(json.dumps(self.release))

    def command(self, *args, env=None):
        clean = {k:v for k,v in os.environ.items() if not k.startswith(('OSS_', 'OSSUTIL_', 'TOKEN_MARKET_UPDATE_', 'TAURI_SIGNING_'))}
        if env:
            clean.update(env)
        return subprocess.run(COMMAND + list(args), text=True, capture_output=True, env=clean)

    @contextlib.contextmanager
    def served_store(self, *, corrupt=False, fault=None):
        store = Path(tempfile.mkdtemp(prefix='store-', dir=self.root))
        observed = []
        self.public_observations = observed
        class Handler(SimpleHTTPRequestHandler):
            def __init__(self, *args, **kwargs):
                super().__init__(*args, directory=str(store), **kwargs)
            def log_message(self, *args):
                pass
            def end_headers(self):
                if fault == 'encoding':
                    self.send_header('Content-Encoding', 'gzip')
                self.send_header('Cache-Control', 'max-age=3600' if fault == 'cache' else 'no-cache, no-store, max-age=0, must-revalidate'
                    if self.path.endswith('stable.json') else 'public, max-age=31536000, immutable')
                super().end_headers()
            def guess_type(self, path):
                if path.endswith('.tar.gz'):
                    return 'application/gzip'
                if path.endswith(('.dmg', '.exe', '.msi')):
                    return 'application/octet-stream'
                return super().guess_type(path)
            def do_GET(self):
                stable = store / 'test/candidate/stable.json'
                observed.append((self.path, stable.read_bytes() if stable.exists() else None))
                if fault == '404' and '/releases/' in self.path:
                    self.send_error(404)
                elif fault == 'truncated' and '/releases/' in self.path:
                    self.send_response(200)
                    self.send_header('Content-Length', '1000')
                    self.end_headers()
                    self.wfile.write(b'truncated')
                    self.close_connection = True
                elif corrupt and '/releases/' in self.path:
                    self.send_response(200)
                    self.end_headers()
                    self.wfile.write(b'corrupt public readback')
                else:
                    super().do_GET()
        server = ThreadingHTTPServer(('127.0.0.1', 0), Handler)
        worker = threading.Thread(target=server.serve_forever, daemon=True)
        worker.start()
        try:
            yield store, f'http://127.0.0.1:{server.server_port}'
        finally:
            server.shutdown()
            server.server_close()
            worker.join()

    def publish(self, store, base, **kwargs):
        verifier = ROOT / 'src-tauri/target/debug/examples/verify-update-artifacts'
        return self.command('publish', '--inventory', str(self.inventory), '--fixture',
            '--local-store', str(store), '--verifier', str(verifier),
            env={'TOKEN_MARKET_UPDATE_PUBLIC_BASE_URL': base}, **kwargs)

    def test_two_target_publication_exposes_only_complete_verified_release_and_recovery(self):
        with self.served_store() as (store, base):
            result = self.publish(store, base)
            self.assertEqual(result.returncode, 0, result.stderr)
            manifest = json.loads((store / 'test/candidate/stable.json').read_text())
            self.assertEqual(manifest['version'], '1.1.0')
            self.assertEqual(set(manifest['platforms']), {'darwin-aarch64', 'windows-x86_64-nsis'})
            recovery = next((store / 'releases/1.1.0').glob('*/recovery.html')).read_text()
            self.assertIn('manual.dmg', recovery)
            self.assertIn('manual.msi', recovery)
            self.assertIn('no automatic rollback', recovery)
            self.assertEqual(self.public_observations[-1][0], '/test/candidate/stable.json')
            self.assertTrue(all(body is None for path, body in self.public_observations[:-1]), 'No public artifact readback may observe advanced discovery')

    def test_prepare_without_private_key_preserves_output_and_suppresses_secrets(self):
        output = self.root / 'prepared'
        result = self.command('prepare', '--version', '1.1.0', '--target', 'darwin-aarch64',
            '--app', str(self.root / 'Token Market.app'), '--output', str(output),
            '--public-key', str(FIXTURES / 'public-key.txt'), '--fixture')
        self.assertNotEqual(result.returncode, 0)
        self.assertIn('TAURI_SIGNING_PRIVATE_KEY', result.stderr)
        self.assertFalse(output.exists())

    def test_prepare_signs_exact_final_installer_bytes_and_checks_signed_version(self):
        signer = self.root / 'fixture-signer'
        signature = (FIXTURES / 'package.txt.sig').read_text().strip()
        signer.write_text('#!' + sys.executable + '\n' +
            'import sys, pathlib\n' +
            'if sys.argv[1:] == ["--version"]: print("tauri-cli 2.12.4")\n' +
            'else:\n' +
            ' assert sys.argv[1:5] == ["signer", "sign", "--app-version", "1.1.0"]\n' +
            ' pathlib.Path(sys.argv[5] + ".sig").write_text(' + repr(signature) + ')\n')
        signer.chmod(0o700)
        output = self.root / 'prepared'
        result = self.command('prepare', '--version', '1.1.0', '--target', 'windows-x86_64-nsis',
            '--app', str(self.root / 'setup.exe'), '--installer', str(self.root / 'setup.exe'),
            '--output', str(output), '--public-key', str(FIXTURES / 'public-key.txt'),
            '--signer', str(signer), '--verifier', str(ROOT / 'src-tauri/target/debug/examples/verify-update-artifacts'),
            '--fixture', env={'TAURI_SIGNING_PRIVATE_KEY': 'fixture-private-env-only'})
        self.assertEqual(result.returncode, 0, result.stderr)
        receipt = json.loads((output / 'prepared.json').read_text())
        self.assertEqual((output / receipt['artifact']).read_bytes(), (FIXTURES / 'package.txt').read_bytes())
        self.assertNotIn('fixture-private-env-only', result.stdout + result.stderr + (output / 'prepared.json').read_text())

    def test_untrusted_or_incomplete_inputs_never_advance_previous_discovery(self):
        original = json.dumps(self.release)
        for failure in ['missing-target', 'missing-signature', 'wrong-signature', 'signed-version-mismatch',
                        'changed-bytes', 'missing-manual', 'production-route', 'fixture-to-oss']:
            with self.subTest(failure=failure), self.served_store() as (store, base):
                stable = store / 'test/candidate/stable.json'
                stable.parent.mkdir(parents=True, exist_ok=True)
                stable.write_bytes(b'previous-release')
                self.release = json.loads(original)
                if failure == 'missing-target':
                    del self.release['targets']['darwin-aarch64']
                elif failure == 'missing-signature':
                    self.release['targets']['darwin-aarch64']['signature'] = ''
                elif failure == 'wrong-signature':
                    self.release['targets']['darwin-aarch64']['signature'] = (FIXTURES / 'wrong-key.sig').read_text().strip()
                elif failure == 'signed-version-mismatch':
                    self.release['version'] = '1.2.0'
                elif failure == 'changed-bytes':
                    (self.root / 'mac.app.tar.gz').write_bytes(b'altered archive bytes')
                elif failure == 'missing-manual':
                    self.release['installers'] = []
                self.save()
                verifier = ROOT / 'src-tauri/target/debug/examples/verify-update-artifacts'
                extra = ['--manifest-key', 'stable.json'] if failure == 'production-route' else []
                command = ['publish', '--inventory', str(self.inventory), '--fixture', '--verifier', str(verifier)]
                if failure != 'fixture-to-oss':
                    command += ['--local-store', str(store)]
                result = self.command(*(command + extra), env={'TOKEN_MARKET_UPDATE_PUBLIC_BASE_URL': base})
                self.assertNotEqual(result.returncode, 0, result.stdout)
                self.assertEqual(stable.read_bytes(), b'previous-release')
                self.assertFalse((store / 'releases').exists(), 'preflight must reject before any upload')
                (self.root / 'mac.app.tar.gz').write_bytes((FIXTURES / 'package.txt').read_bytes())

    def test_corrupt_public_readback_preserves_previous_discovery(self):
        with self.served_store(corrupt=True) as (store, base):
            stable = store / 'test/candidate/stable.json'
            stable.parent.mkdir(parents=True)
            stable.write_bytes(b'previous-release')
            result = self.publish(store, base)
            self.assertNotEqual(result.returncode, 0)
            self.assertEqual(stable.read_bytes(), b'previous-release')

    def test_partial_local_upload_failures_preserve_previous_discovery(self):
        for blocked in ['mac.app.tar.gz', 'setup.exe']:
            with self.subTest(blocked=blocked), self.served_store() as (store, base):
                plan = self.command('plan', '--fixture', '--inventory', str(self.inventory),
                    env={'TOKEN_MARKET_UPDATE_PUBLIC_BASE_URL': base})
                obj = next(o for o in json.loads(plan.stdout)['objects'] if o['path'].endswith(blocked))
                destination = store / obj['key']
                destination.parent.mkdir(parents=True, exist_ok=True)
                destination.mkdir()
                stable = store / 'test/candidate/stable.json'
                stable.parent.mkdir(parents=True, exist_ok=True)
                stable.write_bytes(b'previous-release')
                result = self.publish(store, base)
                self.assertNotEqual(result.returncode, 0)
                self.assertEqual(stable.read_bytes(), b'previous-release')

    def test_child_failure_never_echoes_private_key_or_upload_identity(self):
        verifier = self.root / 'failing-verifier'
        sentinel = 'synthetic-private-secret-do-not-log'
        verifier.write_text('#!' + sys.executable + '\nimport sys\nprint(' + repr(sentinel) + ')\nsys.exit(1)\n')
        verifier.chmod(0o700)
        with self.served_store() as (store, base):
            result = self.command('publish', '--fixture', '--inventory', str(self.inventory),
                '--local-store', str(store), '--verifier', str(verifier),
                env={'TOKEN_MARKET_UPDATE_PUBLIC_BASE_URL': base, 'TAURI_SIGNING_PRIVATE_KEY': sentinel,
                     'OSS_ACCESS_KEY_SECRET': sentinel})
            self.assertNotEqual(result.returncode, 0)
            self.assertNotIn(sentinel, result.stdout + result.stderr)
            self.assertFalse((store / 'releases').exists())

    @unittest.skipUnless(os.environ.get('TOKEN_MARKET_TEST_TAURI_SIGNER') or shutil.which('tauri'),
                         'set TOKEN_MARKET_TEST_TAURI_SIGNER for the disposable real-signer archive check')
    def test_macos_archive_is_signed_after_final_app_copy_with_a_disposable_real_key(self):
        signer = os.environ.get('TOKEN_MARKET_TEST_TAURI_SIGNER') or shutil.which('tauri')
        key = self.root / 'disposable.key'
        generated = subprocess.run([signer, 'signer', 'generate', '--ci', '--password', '', '--write-keys', str(key)],
                                   text=True, capture_output=True)
        self.assertEqual(generated.returncode, 0, 'Disposable fixture key generation failed')
        app = self.root / 'Token Market.app'
        binary = app / 'Contents/MacOS/Token Market'
        binary.parent.mkdir(parents=True)
        binary.write_bytes(b'final-signed-app-fixture-after-platform-signing')
        output = self.root / 'prepared'
        private = key.read_text()
        result = self.command('prepare', '--version', '1.1.0', '--target', 'darwin-aarch64',
            '--app', str(app), '--output', str(output), '--public-key', str(key.with_name(key.name + '.pub')),
            '--signer', str(signer), '--verifier', str(ROOT / 'src-tauri/target/debug/examples/verify-update-artifacts'),
            '--fixture', env={'TAURI_SIGNING_PRIVATE_KEY': private, 'TAURI_SIGNING_PRIVATE_KEY_PASSWORD': ''})
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertNotIn(private, result.stdout + result.stderr)
        with tarfile.open(output / 'TokenMarket.app.tar.gz') as archive:
            self.assertEqual(archive.extractfile('Token Market.app/Contents/MacOS/Token Market').read(),
                             b'final-signed-app-fixture-after-platform-signing')
        self.assertEqual({p.name for p in output.iterdir()}, {'TokenMarket.app.tar.gz', 'TokenMarket.app.tar.gz.sig', 'prepared.json'})

    def test_unavailable_truncated_or_transformed_public_objects_never_advance_discovery(self):
        for fault in ['404', 'truncated', 'encoding', 'cache']:
            with self.subTest(fault=fault), self.served_store(fault=fault) as (store, base):
                stable = store / 'test/candidate/stable.json'
                stable.parent.mkdir(parents=True)
                stable.write_bytes(b'previous-release')
                result = self.publish(store, base)
                self.assertNotEqual(result.returncode, 0)
                self.assertEqual(stable.read_bytes(), b'previous-release')

    def test_oss_adapter_uses_v2_flags_and_suppresses_failed_upload_output(self):
        # The provider executable is a public process boundary, not an internal mock.
        # It records argv and refuses uploads, so this test cannot contact real OSS.
        transcript = self.root / 'oss-argv.json'
        ossutil = self.root / 'ossutil-fixture'
        secret = 'synthetic-provider-private-secret'
        ossutil.write_text('#!' + sys.executable + '\n' +
            'import os,sys,json,pathlib\n' +
            'if sys.argv[1:] == ["version"]: print("2.4.1"); sys.exit(0)\n' +
            'if sys.argv[1:] == ["cp","--help"]: print("--cache-control --content-type --metadata --ignore-existing"); sys.exit(0)\n' +
            'p=pathlib.Path(os.environ["FAKE_OSS_TRANSCRIPT"])\n' +
            'items=json.loads(p.read_text()) if p.exists() else []\n' +
            'items.append(sys.argv[1:]); p.write_text(json.dumps(items))\n' +
            'print(os.environ["OSS_ACCESS_KEY_SECRET"])\n' +
            'sys.exit(1 if len(items) >= int(os.environ["FAKE_OSS_FAIL_AT"]) else 0)\n')
        ossutil.chmod(0o700)
        for target, record in self.release['targets'].items():
            record['platform_signing'] = dict(verified=True,
                kind='codesign' if target == 'darwin-aarch64' else 'authenticode',
                artifact_sha256=hashlib.sha256((self.root / record['artifact']).read_bytes()).hexdigest(),
                bundler_version='2.10.4', nsis_install_mode='currentUser')
        for record in self.release['installers']:
            record['platform_signing'] = dict(verified=True,
                kind='codesign' if record['platform'] == 'darwin-aarch64' else 'authenticode',
                artifact_sha256=hashlib.sha256((self.root / record['path']).read_bytes()).hexdigest())
        self.save()
        result = self.command('publish', '--inventory', str(self.inventory), '--ossutil', str(ossutil),
            '--verifier', str(ROOT / 'src-tauri/target/debug/examples/verify-update-artifacts'),
            env={'OSS_REGION': 'fixture-region', 'OSS_ENDPOINT': 'https://oss.invalid',
                 'TOKEN_MARKET_UPDATE_OSS_BUCKET': 'fixture-bucket', 'TOKEN_MARKET_UPDATE_OSS_PREFIX': 'isolated',
                 'TOKEN_MARKET_UPDATE_PUBLIC_BASE_URL': 'https://127.0.0.1:1',
                 'OSS_ACCESS_KEY_ID': 'fixture', 'OSS_ACCESS_KEY_SECRET': secret, 'OSS_SESSION_TOKEN': 'fixture',
                 'FAKE_OSS_TRANSCRIPT': str(transcript), 'FAKE_OSS_FAIL_AT': '1'})
        self.assertNotEqual(result.returncode, 0)
        self.assertNotIn(secret, result.stdout + result.stderr)
        writes = json.loads(transcript.read_text())
        self.assertTrue(writes)
        self.assertTrue(all('stable.json' not in ' '.join(write) for write in writes))
        upload = writes[0]
        self.assertEqual(upload[0], 'cp')
        self.assertTrue(upload[2].startswith('oss://fixture-bucket/isolated/releases/1.1.0/'))
        self.assertIn('--ignore-existing', upload)
        self.assertIn('--cache-control', upload)
        self.assertIn('--content-type', upload)
        self.assertIn('--metadata', upload)
        self.assertNotIn('--meta', upload)
        self.assertNotIn(secret, ' '.join(upload))

    def test_unconfigured_plan_reports_missing_fields_without_inventing_routes(self):
        result = self.command('plan', '--inventory', str(self.inventory), '--fixture')
        self.assertEqual(result.returncode, 0, result.stderr)
        plan = json.loads(result.stdout)
        self.assertEqual(plan['missing_configuration'], ['OSS_REGION', 'OSS_ENDPOINT',
            'TOKEN_MARKET_UPDATE_OSS_BUCKET', 'TOKEN_MARKET_UPDATE_OSS_PREFIX',
            'TOKEN_MARKET_UPDATE_PUBLIC_BASE_URL'])
        self.assertIsNone(plan['manifest_url'])
        self.assertEqual(sorted(p.name for p in self.root.iterdir()),
            ['mac.app.tar.gz', 'manual.dmg', 'manual.msi', 'release.json', 'setup.exe'])

if __name__ == '__main__':
    unittest.main()
