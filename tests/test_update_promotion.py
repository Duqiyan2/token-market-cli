"""Promotion is exercised through the release CLI and isolated public HTTP route."""
from __future__ import annotations
import json
import hashlib
import contextlib
import copy
import threading
import tempfile
from pathlib import Path
from http.server import SimpleHTTPRequestHandler, ThreadingHTTPServer
import unittest
import test_update_distribution as distribution

class PromotionCommand(unittest.TestCase):
    setUp = distribution.DistributionCommand.setUp
    save = distribution.DistributionCommand.save
    command = distribution.DistributionCommand.command

    checks = (
        'signed_out', 'zero_balance', 'automatic_discovery', 'manual_discovery',
        'release_notes', 'download_consent', 'install_consent', 'later_process_restart',
        'ignore_process_restart', 'ignored_manual_discovery', 'newer_version_reminder',
        'desktop_busy_refusal_retry', 'cli_mutation_refusal_retry', 'guard_transition_race',
        'active_work_not_cancelled', 'client_session_retained', 'credential_identity_retained',
        'tool_connections_retained', 'billing_owner_retained', 'models_retained',
        'user_configuration_retained', 'restoration_records_retained', 'independent_cli_use',
        'network_failure_retry', 'invalid_package_refusal_retry', 'installer_failure_retry',
        'external_recovery_opened', 'legacy_manual_bootstrap_then_update')

    @contextlib.contextmanager
    def served_store(self, fault=None, block_first=False):
        store = self.root / ('store-' + next(tempfile._get_candidate_names()))
        store.mkdir()
        stable = store / 'production/stable.json'
        stable.parent.mkdir()
        stable.write_bytes(b'previous-release')
        self.observed = []
        observed = self.observed
        entered, release = threading.Event(), threading.Event()
        self.readback_entered, self.release_readback = entered, release
        class Handler(SimpleHTTPRequestHandler):
            def __init__(self, *args, **kwargs):
                super().__init__(*args, directory=str(store), **kwargs)
            def log_message(self, *args):
                pass
            def guess_type(self, path):
                if path.endswith('.tar.gz'): return 'application/gzip'
                if path.endswith(('.dmg', '.exe', '.msi')): return 'application/octet-stream'
                return super().guess_type(path)
            def end_headers(self):
                self.send_header('Cache-Control', 'no-cache, no-store, max-age=0, must-revalidate'
                    if self.path.endswith('/stable.json') or self.path == '/recovery/index.html'
                    else 'public, max-age=31536000, immutable')
                super().end_headers()
            def do_GET(self):
                observed.append((self.path, stable.read_bytes()))
                if block_first and len(observed) == 1:
                    entered.set()
                    release.wait(20)
                if fault == 'alias' and self.path == '/recovery/index.html':
                    self.send_error(404)
                elif fault == 'alias-corrupt' and self.path == '/recovery/index.html':
                    self.send_response(200)
                    self.send_header('Content-Type', 'text/html')
                    self.end_headers()
                    self.wfile.write(b'bad recovery alias')
                elif fault == 'manifest' and self.path == '/production/stable.json':
                    self.send_error(503)
                else:
                    super().do_GET()
        server = ThreadingHTTPServer(('127.0.0.1', 0), Handler)
        worker = threading.Thread(target=server.serve_forever, daemon=True)
        worker.start()
        try:
            yield store, 'http://127.0.0.1:' + str(server.server_port)
        finally:
            release.set()
            server.shutdown()
            server.server_close()
            worker.join()

    def acceptance(self, base):
        # Synthetic operator records are allowed only by --fixture --local-store.
        sha = lambda name: hashlib.sha256((self.root / name).read_bytes()).hexdigest()
        artifacts = []
        for target, record in self.release['targets'].items():
            record['platform_signing'] = dict(fixture=True, identity='synthetic-native-identity')
            artifacts.append(dict(role='updater', platform=target, sha256=sha(record['artifact']),
                size=(self.root / record['artifact']).stat().st_size))
        for record in self.release['installers']:
            record['platform_signing'] = dict(fixture=True, identity='synthetic-native-identity')
            artifacts.append(dict(role='manual', platform=record['platform'], sha256=sha(record['path']),
                size=(self.root / record['path']).stat().st_size))
        routes = dict(public_base_url=base, candidate_manifest_url=base + '/test/accepted/stable.json',
            production_manifest_url=base + '/production/stable.json', recovery_url=base + '/recovery/index.html',
            public_key_sha256=hashlib.sha256(self.release['public_key'].encode()).hexdigest())
        manifest = dict(version='1.1.0', notes='Release fixture', platforms={
            target: dict(url=base + '/releases/1.1.0/' + sha(record['artifact']) + '/' + record['artifact'], signature=record['signature'])
            for target, record in self.release['targets'].items()})
        binding = dict(source_commit='a' * 40, version='1.1.0', artifacts=artifacts,
            manifest_sha256=hashlib.sha256((json.dumps(manifest, indent=2) + '\n').encode()).hexdigest())
        devices = {}
        for target, record in self.release['targets'].items():
            target_artifacts = [a for a in artifacts if a['platform'] in
                (target, 'windows-x86_64' if target.startswith('windows') else target)]
            receipt = self.root / (target + '-prepared.json')
            receipt.write_text(json.dumps(dict(version='1.1.0', target=target,
                artifact=record['artifact'], signature=record['signature'],
                platform_signing=record['platform_signing'], installers=[r for r in self.release['installers']
                    if r['platform'] == ('windows-x86_64' if target.startswith('windows') else target)])))
            compiled = dict(endpoint=routes['production_manifest_url'], recovery_url=routes['recovery_url'],
                public_key_sha256=routes['public_key_sha256'])
            policy = dict(version='1.1.0', **compiled)
            provenance = dict(schema_version=2, fixture=True, provenance_kind='local-native-build',
                native_host='disposable-native-adapter', recipe='fixture-adapter',
                configuration_sha256=hashlib.sha256(json.dumps(policy, sort_keys=True, separators=(',', ':')).encode()).hexdigest(),
                started_at='2026-11-04T00:00:00Z', completed_at='2026-11-04T00:00:01Z',
                source_commit='a' * 40, version='1.1.0', target=target,
                artifacts=target_artifacts, prepared_sha256=sha(receipt.name),
                signing_identity='synthetic-native-identity', compiled=compiled)
            path = self.root / (target + '-build.json')
            path.write_text(json.dumps(provenance))
            record['build_provenance'] = path.name
            record['prepared_receipt'] = receipt.name
            devices[target] = dict(actual_device=False, fixture=True, operator='synthetic operator',
                tested_at='2026-11-04T00:00:00Z', os_version='synthetic OS',
                architecture='arm64' if target.startswith('darwin') else 'x86_64',
                starting_version='1.0.0', starting_updater_enabled=True, starting_install_path='/synthetic/installed/old',
                installed_upgrade=dict(status='passed', evidence=['synthetic://installed-upgrade']),
                startup=dict(status='passed', evidence=['synthetic://new-process'], version='1.1.0',
                    executable='/synthetic/installed/new', artifact_sha256=sha(record['artifact'])),
                build_provenance_sha256=sha(path.name), prepared_sha256=sha(receipt.name),
                signing_identity='synthetic-native-identity', compiled=compiled,
                checks={check: dict(status='passed', evidence=['synthetic://' + check]) for check in self.checks})
        self.save()
        accepted = dict(schema_version=2, fixture=True, provenance_kind='local-operator-acceptance', release=binding, routes=routes, devices=devices,
            evidence={name: dict(status='passed', evidence=['synthetic://' + name])
                for name in ('automated', 'native_signing', 'deployment_readback')})
        self.acceptance_path = self.root / 'acceptance.json'
        self.acceptance_path.write_text(json.dumps(accepted))
        return accepted

    def promote(self, store, base, *extra, acceptance=None):
        if acceptance is not None:
            self.acceptance_path.write_text(json.dumps(acceptance))
        return self.command('promote', '--inventory', str(self.inventory), '--acceptance', str(self.acceptance_path),
            '--fixture', '--local-store', str(store), '--verifier',
            str((distribution.ROOT / 'src-tauri/target/debug/examples/verify-update-artifacts').resolve()), *extra,
            env={'TOKEN_MARKET_UPDATE_PUBLIC_BASE_URL': base,
                 'TOKEN_MARKET_UPDATE_PRODUCTION_MANIFEST_KEY': 'production/stable.json',
                 'TOKEN_MARKET_UPDATE_RECOVERY_KEY': 'recovery/index.html',
                 'TOKEN_MARKET_UPDATE_TRUSTED_BUILD_REPOSITORY': 'fixture/project',
                 'TOKEN_MARKET_UPDATE_TRUSTED_BUILD_WORKFLOWS': json.dumps({target: '.github/workflows/native-fixture.yml'
                    for target in self.release['targets']})})

    def test_fixture_promotes_exact_accepted_bytes_with_fixed_recovery_before_discovery(self):
        with self.served_store() as (store, base):
            self.acceptance(base)
            result = self.promote(store, base)
            self.assertEqual(result.returncode, 0, result.stderr)
            self.assertEqual(json.loads((store / 'production/stable.json').read_text())['version'], '1.1.0')
            immutable = next((store / 'releases/1.1.0').glob('*/recovery.html'))
            self.assertEqual((store / 'recovery/index.html').read_bytes(), immutable.read_bytes())
            self.assertEqual([path for path, _ in self.observed][-2:],
                ['/recovery/index.html', '/production/stable.json'])
            self.assertTrue(all(body == b'previous-release' for _, body in self.observed[:-1]))
            self.assertEqual(json.loads(result.stdout)['mode'], 'synthetic-fixture')
            self.assertIs(json.loads(result.stdout)['actual_device_acceptance'], False)

    def test_rejects_incomplete_or_mismatched_acceptance_before_any_upload(self):
        with self.served_store() as (store, base):
            original = self.acceptance(base)
            cases = []
            for target in ('darwin-aarch64', 'windows-x86_64-nsis'):
                for check in self.checks:
                    changed = copy.deepcopy(original)
                    changed['devices'][target]['checks'][check]['status'] = 'pending'
                    cases.append((target + '/' + check, changed))
                for field in original['devices'][target]:
                    changed = copy.deepcopy(original)
                    del changed['devices'][target][field]
                    cases.append((target + '/missing/' + field, changed))
            for field, value in [('source_commit', 'b' * 40), ('version', '1.2.0')]:
                changed = copy.deepcopy(original)
                changed['release'][field] = value
                cases.append((field, changed))
            for index in range(4):
                changed = copy.deepcopy(original)
                changed['release']['artifacts'][index]['sha256'] = '0' * 64
                cases.append(('digest/' + str(index), changed))
            for field in original['routes']:
                changed = copy.deepcopy(original)
                changed['routes'][field] = 'https://mismatched.invalid/value'
                cases.append(('route/' + field, changed))
            changed = copy.deepcopy(original)
            changed['devices']['darwin-aarch64']['startup']['version'] = '1.0.0'
            cases.append(('wrong-new-process', changed))
            changed = copy.deepcopy(original)
            changed['devices']['windows-x86_64-nsis']['checks']['independent_cli_use']['evidence'] = []
            cases.append(('empty-cli-evidence', changed))
            changed = copy.deepcopy(original)
            changed['devices']['darwin-aarch64']['signing_identity'] = 'different'
            cases.append(('identity', changed))
            changed = copy.deepcopy(original)
            changed['devices']['darwin-aarch64']['compiled']['endpoint'] = base + '/test/other/stable.json'
            cases.append(('compiled-test-endpoint', changed))
            for label, changed in cases:
                with self.subTest(case=label):
                    result = self.promote(store, base, acceptance=changed)
                    self.assertNotEqual(result.returncode, 0, result.stdout)
                    self.assertEqual((store / 'production/stable.json').read_bytes(), b'previous-release')
                    self.assertFalse((store / 'releases').exists(), 'acceptance gate must precede any upload')
            self.assertEqual(self.observed, [], 'invalid evidence must not reach even public readback')

    def test_fixture_acceptance_cannot_reach_oss_even_with_complete_provider_settings(self):
        with self.served_store() as (store, base):
            self.acceptance(base)
            transcript = self.root / 'unexpected-provider-execution'
            provider = self.root / 'ossutil'
            provider.write_text('#!/bin/sh\ntouch "' + str(transcript) + '"\nexit 1\n')
            provider.chmod(0o700)
            result = self.command('promote', '--inventory', str(self.inventory),
                '--acceptance', str(self.acceptance_path), '--fixture', '--ossutil', str(provider),
                env={'TOKEN_MARKET_UPDATE_PUBLIC_BASE_URL': base,
                     'TOKEN_MARKET_UPDATE_PRODUCTION_MANIFEST_KEY': 'production/stable.json',
                     'TOKEN_MARKET_UPDATE_RECOVERY_KEY': 'recovery/index.html',
                     'TOKEN_MARKET_UPDATE_TRUSTED_BUILD_REPOSITORY': 'fixture/project',
                     'TOKEN_MARKET_UPDATE_TRUSTED_BUILD_WORKFLOWS': '{}',
                     'OSS_REGION': 'fixture', 'OSS_ENDPOINT': 'https://oss.invalid',
                     'TOKEN_MARKET_UPDATE_OSS_BUCKET': 'fixture', 'TOKEN_MARKET_UPDATE_OSS_PREFIX': 'fixture',
                     'OSS_ACCESS_KEY_ID': 'fixture', 'OSS_ACCESS_KEY_SECRET': 'fixture', 'OSS_SESSION_TOKEN': 'fixture'})
            self.assertNotEqual(result.returncode, 0)
            self.assertIn('cannot reach OSS', result.stderr)
            self.assertFalse(transcript.exists())

    def test_legacy_real_publish_and_ci_promotion_are_disabled(self):
        accepted = self.acceptance('https://updates.invalid')
        for name in ('publish', 'promote'):
            extra = ['--acceptance', str(self.acceptance_path)] if name == 'promote' else ['--verifier', '/missing']
            result = self.command(name, '--inventory', str(self.inventory), *extra, env={'GITHUB_ACTIONS': 'true'})
            self.assertNotEqual(result.returncode, 0)
            self.assertIn('private --config', result.stderr)

    def test_acceptance_template_is_explicitly_pending_and_cannot_promote(self):
        template = distribution.ROOT / 'docs/client-update-acceptance.template.json'
        self.assertTrue(template.is_file())
        record = json.loads(template.read_text())
        self.assertIs(record['fixture'], False)
        for target in ('darwin-aarch64', 'windows-x86_64-nsis'):
            self.assertIs(record['devices'][target]['actual_device'], False)
            self.assertEqual(record['devices'][target]['startup']['status'], 'pending')
            self.assertEqual(set(record['devices'][target]['checks']), set(self.checks))
            self.assertTrue(all(item['status'] == 'pending' for item in record['devices'][target]['checks'].values()))

    def test_read_only_plan_verifies_accepted_bytes_without_writing_or_reading_public_objects(self):
        with self.served_store() as (store, base):
            self.acceptance(base)
            result = self.promote(store, base, '--dry-run')
            self.assertEqual(result.returncode, 0, result.stderr)
            self.assertIs(json.loads(result.stdout)['dry_run'], True)
            self.assertEqual((store / 'production/stable.json').read_bytes(), b'previous-release')
            self.assertFalse((store / 'releases').exists())
            self.assertFalse((store / '.publish-lock').exists())
            self.assertEqual(self.observed, [])

    def test_recovery_alias_upload_or_readback_failure_preserves_previous_discovery(self):
        for fault in ('alias', 'alias-corrupt', 'alias-upload'):
            with self.subTest(fault=fault), self.served_store(fault=fault) as (store, base):
                self.acceptance(base)
                if fault == 'alias-upload':
                    (store / 'recovery/index.html').mkdir(parents=True)
                result = self.promote(store, base)
                self.assertNotEqual(result.returncode, 0, result.stdout)
                self.assertEqual((store / 'production/stable.json').read_bytes(), b'previous-release')
                self.assertNotIn('/production/stable.json', [path for path, _ in self.observed])
                self.assertTrue(next((store / 'releases/1.1.0').glob('*/recovery.html')).is_file())
                self.assertFalse((store / '.publish-lock').exists())

    def test_manifest_readback_failure_reports_discovery_may_have_advanced(self):
        with self.served_store(fault='manifest') as (store, base):
            self.acceptance(base)
            result = self.promote(store, base)
            self.assertNotEqual(result.returncode, 0)
            self.assertIn('discovery may have advanced', result.stderr)
            self.assertEqual(json.loads((store / 'production/stable.json').read_text())['version'], '1.1.0')
            self.assertTrue((store / 'recovery/index.html').is_file())

    def test_untrusted_build_provenance_or_legacy_receipt_never_reaches_upload(self):
        with self.served_store() as (store, base):
            accepted = self.acceptance(base)
            for target in self.release['targets']:
                path = self.root / self.release['targets'][target]['build_provenance']
                original = path.read_text()
                for field, bad in [('source_commit', 'b' * 40), ('version', '1.2.0'), ('workflow', '.github/workflows/untrusted.yml'),
                                   ('repository', 'other/project'), ('run_url', 'https://untrusted.invalid/123'),
                                   ('compiled', {}), ('signing_identity', 'wrong'), ('fixture', False), ('artifacts', [])]:
                    with self.subTest(target=target, field=field):
                        provenance = json.loads(original)
                        provenance[field] = bad
                        path.write_text(json.dumps(provenance))
                        changed = copy.deepcopy(accepted)
                        changed['devices'][target]['build_provenance_sha256'] = hashlib.sha256(path.read_bytes()).hexdigest()
                        result = self.promote(store, base, acceptance=changed)
                        self.assertNotEqual(result.returncode, 0, result.stdout)
                        self.assertFalse((store / 'releases').exists())
                path.write_text(original)
            del self.release['targets']['darwin-aarch64']['build_provenance']
            self.save()
            result = self.promote(store, base, acceptance=accepted)
            self.assertNotEqual(result.returncode, 0)
            self.assertIn('build_provenance', result.stderr)
            self.assertFalse((store / 'releases').exists())

    def test_real_signature_refusal_precedes_any_promotion_write(self):
        with self.served_store() as (store, base):
            self.release['targets']['darwin-aarch64']['signature'] = (distribution.FIXTURES / 'wrong-key.sig').read_text().strip()
            self.acceptance(base)
            result = self.promote(store, base)
            self.assertNotEqual(result.returncode, 0)
            self.assertIn('signature/version verifier', result.stderr)
            self.assertFalse((store / 'releases').exists())
            self.assertEqual((store / 'production/stable.json').read_bytes(), b'previous-release')

    def test_two_cli_publishers_serialize_the_same_destination(self):
        with self.served_store(block_first=True) as (store, base):
            self.acceptance(base)
            first_results = []
            first = threading.Thread(target=lambda: first_results.append(self.promote(store, base)))
            first.start()
            try:
                self.assertTrue(self.readback_entered.wait(15), 'first publisher must reach artifact readback')
                second = self.promote(store, base)
                self.assertNotEqual(second.returncode, 0)
                self.assertIn('Another publisher owns this destination', second.stderr)
                self.assertEqual((store / 'production/stable.json').read_bytes(), b'previous-release')
            finally:
                self.release_readback.set()
                first.join(timeout=20)
            self.assertFalse(first.is_alive())
            self.assertEqual(first_results[0].returncode, 0, first_results[0].stderr)
            self.assertEqual(json.loads((store / 'production/stable.json').read_text())['version'], '1.1.0')

    def test_snapshot_keeps_exact_accepted_bytes_if_original_changes_during_verification(self):
        with self.served_store() as (store, base):
            self.acceptance(base)
            original = (self.root / 'mac.app.tar.gz').read_bytes()
            verifier = (distribution.ROOT / 'src-tauri/target/debug/examples/verify-update-artifacts').resolve()
            wrapper = self.root / 'verifier-wrapper'
            wrapper.write_text('#!' + distribution.sys.executable + '\nimport os,pathlib\n' +
                'pathlib.Path(' + repr(str(self.root / 'mac.app.tar.gz')) + ').write_bytes(b"changed after snapshot")\n' +
                'os.execv(' + repr(str(verifier)) + ', [' + repr(str(verifier)) + '])\n')
            wrapper.chmod(0o700)
            result = self.promote(store, base, '--verifier', str(wrapper))
            self.assertEqual(result.returncode, 0, result.stderr)
            manifest = json.loads((store / 'production/stable.json').read_text())
            candidate = manifest['platforms']['darwin-aarch64']['url'][len(base) + 1:]
            self.assertEqual((store / candidate).read_bytes(), original)
            self.assertEqual((self.root / 'mac.app.tar.gz').read_bytes(), b'changed after snapshot')

    def test_read_only_promotion_requires_explicit_destination_configuration(self):
        result = self.command('promote', '--inventory', str(self.inventory),
            '--acceptance', str(self.root / 'acceptance.json'), '--dry-run')
        self.assertNotEqual(result.returncode, 0)
        self.assertIn('manifest_key', result.stderr)
        self.assertIn('recovery_key', result.stderr)
        self.assertFalse((self.root / 'releases').exists())

if __name__ == '__main__':
    unittest.main()
