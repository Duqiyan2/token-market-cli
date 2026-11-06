"""Public CLI + isolated local object store + real updater signature verifier."""
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
import json
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile
import threading
import unittest

ROOT = Path(__file__).resolve().parents[1]
SCRIPT = ROOT / 'scripts/release-desktop.py'
FIXTURES = ROOT / 'src-tauri/tests/fixtures/client-update'

class PublishingCommand(unittest.TestCase):
    def setUp(self):
        temp = tempfile.TemporaryDirectory()
        self.addCleanup(temp.cleanup)
        self.root = Path(temp.name)
        self.stage = self.root / 'stage'
        self.store = self.root / 'store'
        self.store.mkdir()
        self.corrupt = None
        owner = self
        class Handler(BaseHTTPRequestHandler):
            def log_message(self, *args):
                pass
            def do_GET(self):
                key = self.path.lstrip('/')
                file = owner.store / key
                if not file.is_file():
                    self.send_error(404)
                    return
                data = file.read_bytes()
                if owner.corrupt == key:
                    data = b'corrupt-public-response'
                values = json.loads((owner.store / 'metadata.json').read_text()) if (owner.store / 'metadata.json').exists() else {}
                headers = values.get(key, {})
                self.send_response(200)
                self.send_header('Content-Type', headers.get('content_type', 'application/octet-stream'))
                self.send_header('Cache-Control', headers.get('cache_control', ''))
                self.send_header('Content-Disposition', headers.get('content_disposition', ''))
                self.send_header('Content-Length', str(len(data)))
                self.end_headers()
                self.wfile.write(data)
        self.server = ThreadingHTTPServer(('127.0.0.1', 0), Handler)
        thread = threading.Thread(target=self.server.serve_forever, daemon=True)
        thread.start()
        self.addCleanup(self.server.server_close)
        self.addCleanup(self.server.shutdown)
        self.base = 'http://127.0.0.1:' + str(self.server.server_port)
        self.plan_path = self.stage / 'upload-map.json'
        target = Path(__import__('os').environ.get('CARGO_TARGET_DIR', ROOT / 'src-tauri/target')).resolve()
        self.config = dict(schema_version=1, public_base_url=self.base + '/desktop-updates', manifest_key='production/stable.json',
            public_key_file=str(FIXTURES / 'public-key.txt'), tools={'verifier': str(target / 'debug/examples/verify-update-artifacts')},
            oss={'bucket':'fixture-bucket','prefix':'desktop-updates'})
        # Public test key is copied outside Git, as required for release configuration.
        public_key = self.root / 'public.key'
        shutil.copy2(FIXTURES / 'public-key.txt', public_key)
        self.config['public_key_file'] = str(public_key)
        self.config_path = self.root / 'config.json'
        self.save_config()
        self.macdir, self.windir = self.root / 'mac', self.root / 'windows'
        self.macdir.mkdir(); self.windir.mkdir()
        import hashlib
        payload = (FIXTURES / 'package.txt').read_bytes()
        checksum = hashlib.sha256(payload).hexdigest()
        signature = (FIXTURES / 'package.txt.sig').read_text().strip()
        for folder, names in [(self.macdir, ['TokenMarket.app.tar.gz','manual.dmg']), (self.windir,['setup.exe'])]:
            for name in names: (folder / name).write_bytes(payload)
        (self.windir / 'setup.exe.sig').write_text(signature)
        self.mac_record = dict(version='1.1.0', target='darwin-aarch64', fixture=True, artifact='TokenMarket.app.tar.gz', signature=signature,
            platform_signing={'artifact_sha256':checksum}, installers=[{'path':'manual.dmg','platform_signing':{'artifact_sha256':checksum}}])
        self.mac_build = dict(version='1.1.0', fixture=True, source_commit='a'*40)
        (self.macdir / 'prepared.json').write_text(json.dumps(self.mac_record))
        (self.macdir / 'build.json').write_text(json.dumps(self.mac_build))
        (self.windir / 'cross-build-verification.json').write_text(json.dumps(dict(version='1.1.0',fixture=True,source_commit='a'*40,
            artifact_sha256=checksum,authenticode_signed=False)))
        result = self.stage_command()
        self.assertEqual(result.returncode, 0, result.stderr)
        self.plan = json.loads(self.plan_path.read_text())

    def stage_command(self, output=None):
        return subprocess.run([sys.executable, str(SCRIPT), 'stage', '--fixture', '--local-store', str(self.store),
            '--config', str(self.config_path), '--mac-dir', str(self.macdir), '--windows-dir', str(self.windir),
            '--output', str(output or self.stage)], capture_output=True, text=True)

    def save_config(self):
        self.config_path.write_text(json.dumps(self.config))

    def save_plan(self):
        self.plan_path.write_text(json.dumps(self.plan))

    def command(self, action, *args):
        return subprocess.run([sys.executable, str(SCRIPT), action, '--fixture', '--local-store', str(self.store),
            '--config', str(self.config_path), '--plan', str(self.plan_path), *args], capture_output=True, text=True)

    def test_complete_publication_writes_stable_last_and_repairs_metadata_without_changing_bytes(self):
        result = self.command('publish')
        self.assertEqual(result.returncode, 0, result.stderr)
        operations = [json.loads(v) for v in (self.store / 'operations.jsonl').read_text().splitlines()]
        self.assertEqual(operations[-1]['key'], 'desktop-updates/production/stable.json')
        self.assertTrue(all(v['key'].startswith('releases/1.1.0/') for v in operations[:5]))
        self.assertEqual(self.command('check').returncode, 0)
        before = (self.store / 'TokenMarket-windows-x64.exe').read_bytes()
        metadata = json.loads((self.store / 'metadata.json').read_text())
        metadata['TokenMarket-windows-x64.exe']['cache_control'] = ''
        (self.store / 'metadata.json').write_text(json.dumps(metadata))
        self.assertNotEqual(self.command('check').returncode, 0)
        repaired = self.command('metadata', '--key', 'TokenMarket-windows-x64.exe')
        self.assertEqual(repaired.returncode, 0, repaired.stderr)
        self.assertEqual((self.store / 'TokenMarket-windows-x64.exe').read_bytes(), before)
        self.assertEqual(self.command('check').returncode, 0)

    def test_corrupt_public_version_readback_never_publishes_stable_or_fixed_aliases(self):
        self.corrupt = 'releases/1.1.0/TokenMarket-mac-arm64.app.tar.gz'
        result = self.command('publish')
        self.assertNotEqual(result.returncode, 0)
        self.assertFalse((self.store / 'desktop-updates/production/stable.json').exists())
        self.assertFalse((self.store / 'TokenMarket-windows-x64.exe').exists())

    def test_download_filenames_include_version_without_changing_urls_or_installer_bytes(self):
        result = self.command('publish')
        self.assertEqual(result.returncode, 0, result.stderr)
        values = json.loads((self.store / 'metadata.json').read_text())
        for key, filename in [
            ('TokenMarket-mac-arm64.dmg', 'TokenMarket-1.1.0-mac-arm64.dmg'),
            ('TokenMarket-windows-x64.exe', 'TokenMarket-1.1.0-windows-x64.exe'),
            ('releases/1.1.0/TokenMarket-mac-arm64.dmg', 'TokenMarket-1.1.0-mac-arm64.dmg'),
            ('releases/1.1.0/TokenMarket-windows-x64.exe', 'TokenMarket-1.1.0-windows-x64.exe'),
        ]:
            self.assertEqual(values[key]['content_disposition'], 'attachment; filename="' + filename + '"')
        key = 'TokenMarket-windows-x64.exe'
        before = (self.store / key).read_bytes()
        values[key]['content_disposition'] = 'attachment; filename="TokenMarket-windows-x64.exe"'
        (self.store / 'metadata.json').write_text(json.dumps(values))
        checked = self.command('check')
        self.assertNotEqual(checked.returncode, 0)
        self.assertIn('download filename header differs', checked.stderr)
        repaired = self.command('metadata', '--key', key)
        self.assertEqual(repaired.returncode, 0, repaired.stderr)
        self.assertEqual((self.store / key).read_bytes(), before)

    def test_newer_stable_prevents_old_release_writes(self):
        path = self.store / 'desktop-updates/production/stable.json'
        path.parent.mkdir(parents=True)
        path.write_text(json.dumps({'version':'1.1.1','notes':'Newer release','platforms':{
            'darwin-aarch64':{'url':'https://example.invalid/mac','signature':'previous'},
            'windows-x86_64-nsis':{'url':'https://example.invalid/win','signature':'previous'}}}))
        previous = path.read_bytes()
        result = self.command('publish')
        self.assertNotEqual(result.returncode, 0)
        self.assertIn('newer stable', result.stderr)
        self.assertEqual(path.read_bytes(), previous)
        self.assertFalse((self.store / 'operations.jsonl').exists())

    def test_failed_final_manifest_readback_reports_live_state_may_have_advanced(self):
        self.corrupt = 'desktop-updates/production/stable.json'
        result = self.command('publish')
        self.assertNotEqual(result.returncode, 0)
        self.assertIn('live discovery may have advanced', result.stderr)
        self.assertTrue((self.store / 'desktop-updates/production/stable.json').is_file())

    def test_version_object_collision_is_not_overwritten(self):
        path = self.store / 'releases/1.1.0/TokenMarket-mac-arm64.app.tar.gz'
        path.parent.mkdir(parents=True)
        path.write_bytes(b'previous immutable content')
        result = self.command('publish')
        self.assertNotEqual(result.returncode, 0)
        self.assertEqual(path.read_bytes(), b'previous immutable content')
        self.assertFalse((self.store / 'desktop-updates/production/stable.json').exists())

    def test_changed_local_bytes_stop_before_any_store_write(self):
        (self.stage / 'public/TokenMarket-windows-x64.exe').write_bytes(b'altered installer')
        result = self.command('publish')
        self.assertNotEqual(result.returncode, 0)
        self.assertIn('Staged file bytes changed', result.stderr)
        self.assertFalse((self.store / 'operations.jsonl').exists())

    def test_private_or_traversing_object_keys_are_rejected(self):
        self.plan['objects'][0]['object_key'] = '../keys/updater.key'
        self.save_plan()
        result = self.command('publish')
        self.assertNotEqual(result.returncode, 0)
        self.assertIn('approved public objects', result.stderr)
        self.assertFalse((self.store / 'operations.jsonl').exists())

    def test_child_secret_output_is_suppressed(self):
        verifier = self.root / 'failing-verifier'
        sentinel = 'synthetic-secret-must-not-appear'
        verifier.write_text('#!' + sys.executable + '\nprint("' + sentinel + '")\nraise SystemExit(1)\n')
        verifier.chmod(0o700)
        self.config['tools']['verifier'] = str(verifier)
        self.save_config()
        result = self.command('publish')
        self.assertNotEqual(result.returncode, 0)
        self.assertNotIn(sentinel, result.stdout + result.stderr)
        self.assertFalse((self.store / 'operations.jsonl').exists())

    def test_staging_preserves_exact_signer_sidecar_bytes_and_versioned_names(self):
        self.assertEqual((self.stage / 'public/releases/1.1.0/TokenMarket-windows-x64.exe.sig').read_bytes(),
                         (self.windir / 'setup.exe.sig').read_bytes())
        self.assertTrue((self.stage / 'public/TokenMarket-mac-arm64.dmg').is_file())
        manifest = json.loads((self.stage / 'public/desktop-updates/production/stable.json').read_text())
        self.assertEqual(manifest['platforms']['windows-x86_64-nsis']['url'],
                         self.base + '/releases/1.1.0/TokenMarket-windows-x64.exe')

    def test_platform_source_mismatch_does_not_create_staging_output(self):
        file = self.windir / 'cross-build-verification.json'
        value = json.loads(file.read_text()); value['source_commit'] = 'b'*40; file.write_text(json.dumps(value))
        output = self.root / 'rejected'
        result = self.stage_command(output)
        self.assertNotEqual(result.returncode, 0)
        self.assertFalse(output.exists())

    def test_fixture_records_cannot_be_reclassified_as_real_publication(self):
        self.plan['fixture'] = False
        self.save_plan()
        result = self.command('publish')
        self.assertNotEqual(result.returncode, 0)
        self.assertIn('fixture mismatch', result.stderr)
        self.assertFalse((self.store / 'operations.jsonl').exists())

if __name__ == '__main__':
    unittest.main()
