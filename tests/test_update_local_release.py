"""Local operator workflow, exercised only through the public subprocess CLI."""
import copy
import contextlib
import hashlib
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import unittest
import test_update_distribution as distribution
import test_update_promotion as promotion


class LocalReleaseCommand(unittest.TestCase):
    setUp = distribution.DistributionCommand.setUp
    save = distribution.DistributionCommand.save
    command = distribution.DistributionCommand.command
    served_store = promotion.PromotionCommand.served_store
    acceptance = promotion.PromotionCommand.acceptance
    checks = promotion.PromotionCommand.checks

    def config(self, base='https://updates.invalid'):
        self.config_path = self.root / 'private.json'
        public = self.root / 'public.key'
        public.write_text(self.release['public_key'])
        profile = self.root / 'oss-profile'
        profile.write_text('disposable isolated profile')
        self.settings = dict(schema_version=1, source_root=str(distribution.ROOT), version='1.1.0',
            public_base_url=base, manifest_key='production/stable.json', recovery_key='recovery/index.html',
            candidate_key='test/accepted/stable.json', public_key_file=str(public),
            private_key_file=str(self.root / 'missing-private.key'), private_key_password_file='',
            output_dir=str(self.root / 'outputs'), state_dir=str(self.root / 'state'),
            tools=dict(signer=os.environ.get('TOKEN_MARKET_TEST_TAURI_SIGNER', '/missing/signer'),
                verifier=str((distribution.ROOT / 'src-tauri/target/debug/examples/verify-update-artifacts').resolve())),
            oss=dict(region='isolated-region', endpoint='https://oss.invalid', bucket='isolated-bucket', prefix='desktop',
                config_file=str(profile), profile='isolated', ossutil='/missing/ossutil'),
            native=dict(macos_identity_dir='', tauri_cli_js='', windows_certificate_sha1='',
                windows_timestamp_url='', windows_signtool=''))
        self.write_config()
        return self.settings

    def write_config(self):
        self.config_path.write_text(json.dumps(self.settings))

    def local(self, command, *args, fixture=True, env=None):
        return self.command('local-' + command, '--config', str(self.config_path),
            *(['--fixture'] if fixture else []), *args, env=env)

    def local_records(self, base):
        accepted = self.acceptance(base)
        accepted.update(schema_version=2, provenance_kind='local-operator-acceptance')
        for target, record in self.release['targets'].items():
            path = self.root / record['build_provenance']
            previous = json.loads(path.read_text())
            local = {k: v for k, v in previous.items() if k not in ('repository', 'workflow', 'run_url', 'run_attempt')}
            local.update(schema_version=2, provenance_kind='local-native-build', native_host='disposable-native-adapter',
                recipe='fixture-adapter', configuration_sha256=self.policy_sha(),
                started_at='2026-11-04T00:00:00Z', completed_at='2026-11-04T00:00:01Z')
            path.write_text(json.dumps(local))
            accepted['devices'][target]['build_provenance_sha256'] = hashlib.sha256(path.read_bytes()).hexdigest()
        self.acceptance_path.write_text(json.dumps(accepted))
        return accepted

    def policy_sha(self):
        policy = dict(version=self.settings['version'], endpoint=self.settings['public_base_url'] + '/' + self.settings['manifest_key'],
            recovery_url=self.settings['public_base_url'] + '/' + self.settings['recovery_key'],
            public_key_sha256=hashlib.sha256(self.release['public_key'].encode()).hexdigest())
        return hashlib.sha256(json.dumps(policy, sort_keys=True, separators=(',', ':')).encode()).hexdigest()

    def test_local_candidate_and_read_only_acceptance_need_no_github_and_preserve_stable(self):
        with self.served_store() as (store, base):
            self.config(base)
            self.local_records(base)
            result = self.local('candidate', '--inventory', str(self.inventory), '--local-store', str(store),
                env={'OSS_ACCESS_KEY_SECRET': 'ambient-secret-must-not-leak', 'GITHUB_ACTIONS': 'true'})
            self.assertEqual(result.returncode, 0, result.stderr)
            self.assertEqual(json.loads((store / 'test/accepted/stable.json').read_text())['version'], '1.1.0')
            self.assertEqual((store / 'production/stable.json').read_bytes(), b'previous-release')
            self.assertNotIn('ambient-secret-must-not-leak', result.stdout + result.stderr)
            self.observed.clear()
            self.settings['oss'] = {field: '' for field in self.settings['oss']}
            self.settings['tools']['signer'] = ''
            for field in ('private_key_file', 'source_root', 'output_dir', 'state_dir'):
                self.settings[field] = ''
            self.write_config()
            result = self.local('validate', '--inventory', str(self.inventory), '--acceptance', str(self.acceptance_path),
                '--local-store', str(store))
            self.assertEqual(result.returncode, 0, result.stderr)
            self.assertIs(json.loads(result.stdout)['dry_run'], True)
            self.assertEqual(self.observed, [])

    def test_local_publish_reads_recovery_before_advancing_stable_and_refuses_stale_or_ambiguous_current_release(self):
        with self.served_store() as (store, base):
            self.config(base)
            self.local_records(base)
            stable = store / 'production/stable.json'
            current_manifest = dict(version='1.2.0', notes='previous', platforms={target: dict(url=base + '/old', signature='old') for target in self.release['targets']})
            for previous, expected in [(json.dumps(current_manifest).encode(), 'newer'), (b'malformed', 'current production')]:
                stable.write_bytes(previous)
                result = self.local('publish', '--inventory', str(self.inventory), '--acceptance', str(self.acceptance_path), '--local-store', str(store))
                self.assertNotEqual(result.returncode, 0)
                self.assertIn(expected, result.stderr)
                self.assertEqual(stable.read_bytes(), previous)
                self.assertFalse((store / 'releases').exists())
            current_manifest['version'] = '1.0.0'
            stable.write_bytes(json.dumps(current_manifest).encode())
            result = self.local('publish', '--inventory', str(self.inventory), '--acceptance', str(self.acceptance_path), '--local-store', str(store))
            self.assertEqual(result.returncode, 0, result.stderr)
            self.assertEqual([path for path, _ in self.observed][-2:], ['/recovery/index.html', '/production/stable.json'])
            current = stable.read_bytes()
            self.release['notes'] = 'Changed notes at same version'
            self.save()
            self.local_records(base)
            result = self.local('publish', '--inventory', str(self.inventory), '--acceptance', str(self.acceptance_path), '--local-store', str(store))
            self.assertNotEqual(result.returncode, 0)
            self.assertEqual(stable.read_bytes(), current)

    def test_local_private_evidence_is_bound_by_hash_and_old_ci_or_fixture_records_are_rejected(self):
        self.config()
        accepted = self.local_records(self.settings['public_base_url'])
        evidence = self.root / 'acceptance-notes.txt'
        evidence.write_text('Disposable local acceptance evidence')
        accepted['evidence']['automated']['evidence'] = [dict(path=str(evidence), sha256=hashlib.sha256(evidence.read_bytes()).hexdigest())]
        self.acceptance_path.write_text(json.dumps(accepted))
        args = ('--inventory', str(self.inventory), '--acceptance', str(self.acceptance_path), '--local-store', str(self.root / 'isolated-store'))
        result = self.local('validate', *args)
        self.assertEqual(result.returncode, 0, result.stderr)
        evidence.write_text('Changed evidence after acceptance')
        result = self.local('validate', *args)
        self.assertNotEqual(result.returncode, 0)
        self.assertIn('evidence digest mismatch', result.stderr)
        self.acceptance(self.settings['public_base_url'])
        for record in self.release['targets'].values():
            path = self.root / record['build_provenance']
            value = json.loads(path.read_text())
            value.update(schema_version=1, workflow='.github/workflows/legacy.yml')
            path.write_text(json.dumps(value))  # retired CI provenance
        result = self.local('candidate', '--inventory', str(self.inventory), '--local-store', str(self.root / 'isolated-store'))
        self.assertNotEqual(result.returncode, 0)
        self.assertIn('local build provenance', result.stderr)
        self.assertFalse((self.root / 'isolated-store').exists())

    def test_local_candidate_and_stable_share_a_process_lock_across_private_state_and_workdirs(self):
        import threading
        with self.served_store(block_first=True) as (store, base):
            self.config(base)
            self.local_records(base)
            results = []
            first = threading.Thread(target=lambda: results.append(self.local('candidate', '--inventory', str(self.inventory), '--local-store', str(store))))
            first.start()
            try:
                self.assertTrue(self.readback_entered.wait(15))
                second_config = copy.deepcopy(self.settings)
                second_config['state_dir'] = str(self.root / 'other-private-state')
                other = self.root / 'second-config.json'
                other.write_text(json.dumps(second_config))
                result = self.command('local-publish', '--config', str(other), '--fixture', '--local-store', str(store),
                    '--inventory', str(self.inventory), '--acceptance', str(self.acceptance_path))
                self.assertNotEqual(result.returncode, 0)
                self.assertIn('Another publisher owns this destination', result.stderr)
                self.assertEqual((store / 'production/stable.json').read_bytes(), b'previous-release')
            finally:
                self.release_readback.set()
                first.join(20)
            self.assertFalse(first.is_alive())
            self.assertEqual(results[0].returncode, 0, results[0].stderr)

    @unittest.skipUnless(os.environ.get('TOKEN_MARKET_TEST_TAURI_SIGNER'), 'set disposable real signer')
    def test_native_build_invocation_prepares_both_targets_and_collects_inventory_without_signature_editing(self):
        stack = contextlib.ExitStack()
        self.addCleanup(stack.close)
        store, base = stack.enter_context(self.served_store())
        self.config(base)
        signer = self.settings['tools']['signer']
        private = self.root / 'disposable.key'
        generated = subprocess.run([signer, 'signer', 'generate', '--ci', '--password', '', '--write-keys', str(private)], capture_output=True)
        self.assertEqual(generated.returncode, 0)
        self.settings['public_key_file'] = str(private.with_name(private.name + '.pub'))
        self.settings['private_key_file'] = str(private)
        source = self.root / 'source'
        (source / 'src-tauri').mkdir(parents=True)
        (source / 'src-tauri/tauri.conf.json').write_text(json.dumps(dict(version='1.1.0')))
        (source / 'src-tauri/Cargo.toml').write_text('[package]\nname = "app"\nversion = "1.1.0"\n')
        for argv in (['init', '-q'], ['add', '.'], ['-c', 'user.name=Fixture', '-c', 'user.email=fixture@invalid', 'commit', '-qm', 'fixture']):
            subprocess.run(['git', '-C', str(source)] + argv, check=True, capture_output=True)
        self.settings['source_root'] = str(source)
        # Native tool adapter is allowed only in explicit fixture mode. It creates
        # synthetic final app/installers; the updater signatures/verifier are real.
        adapter = self.root / 'native-adapter.py'
        adapter.write_text("import pathlib,sys,json,os\nroot=pathlib.Path(sys.argv[1]);root.mkdir(parents=True)\n"
            "assert not any(k.startswith(('OSS_', 'OSSUTIL_', 'GITHUB_', 'TAURI_SIGNING_')) for k in os.environ)\n"
            "if sys.argv[2]=='darwin-aarch64':\n p=root/'Token Market.app/Contents/MacOS/Token Market';p.parent.mkdir(parents=True);p.write_bytes(b'final synthetic app');(root/'manual.dmg').write_bytes(b'manual dmg')\n"
            "else:\n [(root/name).write_bytes(b'final synthetic installer') for name in ('TokenMarket.exe','setup.exe','manual.msi')]\n"
            "(root/'signing-manifest.json').write_text(json.dumps({'fixture':True}))\n")
        self.write_config()
        receipts = []
        for target in ('darwin-aarch64', 'windows-x86_64-nsis'):
            result = self.local('build', '--target', target, '--fixture-native-command', sys.executable, str(adapter),
                env={'OSS_ACCESS_KEY_SECRET': 'ambient-upload-secret', 'TAURI_SIGNING_PRIVATE_KEY': 'ambient-wrong-key'})
            self.assertEqual(result.returncode, 0, result.stderr)
            result = json.loads(result.stdout)
            receipts.append(result['prepared_receipt'])
            record = json.loads(Path(result['build_provenance']).read_text())
            self.assertEqual(record['provenance_kind'], 'local-native-build')
            self.assertEqual(record['source_commit'], subprocess.check_output(['git', '-C', str(source), 'rev-parse', 'HEAD'], text=True).strip())
            self.assertNotIn(private.read_text(), Path(result['build_provenance']).read_text())
        notes = self.root / 'notes.txt'; notes.write_text('Local fixture release notes')
        result = self.local('inventory', '--prepared', receipts[0], '--prepared', receipts[1], '--notes-file', str(notes))
        self.assertEqual(result.returncode, 0, result.stderr)
        result = json.loads(result.stdout)
        draft = json.loads(Path(result['acceptance_template']).read_text())
        self.assertTrue(all(device['actual_device'] is False for device in draft['devices'].values()))
        self.assertTrue(all(device['startup']['status'] == 'pending' for device in draft['devices'].values()))
        inventory = Path(result['inventory'])
        collected = json.loads(inventory.read_text())
        self.assertEqual(set(collected['targets']), {'darwin-aarch64', 'windows-x86_64-nsis'})
        self.assertEqual({item['platform'] for item in collected['installers']}, {'darwin-aarch64', 'windows-x86_64'})
        result = self.local('candidate', '--inventory', str(inventory), '--local-store', str(store))
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(json.loads((store / 'test/accepted/stable.json').read_text())['version'], '1.1.0')
        self.assertEqual((store / 'production/stable.json').read_bytes(), b'previous-release')

    def test_local_oss_adapter_pins_profile_and_v2_flags_and_suppresses_provider_output(self):
        # A disposable provider executable refuses the first write; no OSS client
        # or real network destination can be reached by this subprocess test.
        self.config('https://127.0.0.1:1')
        self.local_records(self.settings['public_base_url'])
        identity = 'c' * 40
        for target, record in self.release['targets'].items():
            record['platform_signing'] = dict(verified=True, certificate_sha1=identity,
                kind='codesign' if target.startswith('darwin') else 'authenticode',
                artifact_sha256=hashlib.sha256((self.root / record['artifact']).read_bytes()).hexdigest(),
                notarized=False, bundler_version='2.10.4', nsis_install_mode='currentUser')
        for item in self.release['installers']:
            item['platform_signing'] = dict(verified=True, certificate_sha1=identity,
                kind='codesign' if item['platform'].startswith('darwin') else 'authenticode',
                artifact_sha256=hashlib.sha256((self.root / item['path']).read_bytes()).hexdigest())
        for target, record in self.release['targets'].items():
            receipt = self.root / record['prepared_receipt']
            receipt.write_text(json.dumps(dict(version='1.1.0', target=target, artifact=record['artifact'],
                signature=record['signature'], platform_signing=record['platform_signing'],
                installers=[item for item in self.release['installers'] if item['platform'] == ('windows-x86_64' if target.startswith('windows') else target)])))
            path = self.root / record['build_provenance']; provenance = json.loads(path.read_text())
            provenance.update(fixture=False, recipe='build-macos-clients.sh' if target.startswith('darwin') else 'build-windows-clients.ps1',
                prepared_sha256=hashlib.sha256(receipt.read_bytes()).hexdigest(), signing_identity=identity)
            path.write_text(json.dumps(provenance))
        self.save()
        transcript = self.root / 'provider.json'
        provider = self.root / 'ossutil'
        provider.write_text('#!' + sys.executable + '\nimport os,sys,json,pathlib\n'
            'if sys.argv[1:]==["version"]: print("2.4.1");sys.exit(0)\n'
            'if sys.argv[1:]==["cp","--help"]: print("--cache-control --content-type --metadata --ignore-existing");sys.exit(0)\n'
            'pathlib.Path(' + repr(str(transcript)) + ').write_text(json.dumps({"args":sys.argv[1:],"profile":os.environ.get("OSSUTIL_PROFILE"),"config":os.environ.get("OSSUTIL_CONFIG_FILE"),"ambient_access_key":os.environ.get("OSS_ACCESS_KEY_SECRET")}))\n'
            'print("provider-secret-sentinel");sys.exit(1)\n')
        provider.chmod(0o700)
        self.settings['oss']['ossutil'] = str(provider)
        self.write_config()
        result = self.local('candidate', '--inventory', str(self.inventory), fixture=False,
            env={'OSSUTIL_PROFILE': 'wrong-ambient-profile', 'OSS_ACCESS_KEY_SECRET': 'ambient-secret'})
        self.assertNotEqual(result.returncode, 0)
        self.assertIn('OSS object upload failed', result.stderr)
        self.assertNotIn('provider-secret-sentinel', result.stdout + result.stderr)
        observed = json.loads(transcript.read_text())
        self.assertEqual(observed['profile'], 'isolated')
        self.assertEqual(observed['config'], str(Path(self.settings['oss']['config_file']).resolve()))
        self.assertIsNone(observed['ambient_access_key'])
        self.assertEqual(observed['args'][0], 'cp')
        self.assertTrue(observed['args'][2].startswith('oss://isolated-bucket/desktop/releases/1.1.0/'))
        self.assertIn('--ignore-existing', observed['args'])
        self.assertIn('--cache-control', observed['args'])
        self.assertIn('--content-type', observed['args'])
        self.assertIn('--metadata', observed['args'])
        self.assertNotIn('stable.json', ' '.join(observed['args']))

    def test_native_version_mismatch_is_rejected_before_native_command(self):
        self.config()
        source = self.root / 'mismatched-source'
        (source / 'src-tauri').mkdir(parents=True)
        (source / 'src-tauri/tauri.conf.json').write_text('{"version":"1.1.0"}')
        (source / 'src-tauri/Cargo.toml').write_text('[package]\nname="app"\nversion="1.0.0"\n')
        for argv in (['init', '-q'], ['add', '.'], ['-c', 'user.name=Fixture', '-c', 'user.email=fixture@invalid', 'commit', '-qm', 'fixture']):
            subprocess.run(['git', '-C', str(source)] + argv, check=True, capture_output=True)
        self.settings['source_root'] = str(source)
        self.settings['oss'] = {field: '' for field in self.settings['oss']}  # native builds need no OSS identity
        self.write_config()
        marker = self.root / 'native-must-not-run'
        adapter = self.root / 'should-not-run.py'
        adapter.write_text('from pathlib import Path\nPath(' + repr(str(marker)) + ').touch()\n')
        result = self.local('build', '--target', 'darwin-aarch64', '--fixture-native-command', sys.executable, str(adapter))
        self.assertNotEqual(result.returncode, 0)
        self.assertIn('src-tauri/Cargo.toml', result.stderr)
        self.assertFalse(marker.exists())

    def test_release_cli_without_cache_environment_keeps_a_clean_source_checkout_clean(self):
        self.config()
        source = self.root / 'clean-source'
        (source / 'scripts').mkdir(parents=True)
        (source / 'src-tauri').mkdir()
        for name in ('desktop-update-release.py', 'desktop_update_local.py'):
            shutil.copyfile(distribution.ROOT / 'scripts' / name, source / 'scripts' / name)
        (source / 'src-tauri/tauri.conf.json').write_text('{"version":"1.1.0"}')
        (source / 'src-tauri/Cargo.toml').write_text('[package]\nname="app"\nversion="1.1.0"\n')
        for argv in (['init', '-q'], ['add', '.'], ['-c', 'user.name=Fixture', '-c', 'user.email=fixture@invalid', 'commit', '-qm', 'fixture']):
            subprocess.run(['git', '-C', str(source)] + argv, check=True, capture_output=True)
        self.settings['source_root'] = str(source)
        self.write_config()
        env = dict(os.environ)
        env.pop('PYTHONPYCACHEPREFIX', None)
        env.pop('PYTHONDONTWRITEBYTECODE', None)
        result = subprocess.run([sys.executable, '-X', 'pycache_prefix=', str(source / 'scripts/desktop-update-release.py'), 'local-build', '--config', str(self.config_path),
            '--fixture', '--target', 'darwin-aarch64', '--fixture-native-command', '/missing/native'], text=True, capture_output=True, env=env)
        self.assertNotEqual(result.returncode, 0)
        self.assertIn('Missing private_key_file', result.stderr)
        self.assertEqual(subprocess.check_output(['git', '-C', str(source), 'status', '--porcelain'], text=True), '')
        self.assertFalse((source / 'scripts/__pycache__').exists())

    @unittest.skipUnless(os.environ.get('TOKEN_MARKET_TEST_TAURI_SIGNER'), 'set disposable real signer')
    def test_failed_or_dirty_native_source_cannot_emit_success_or_leak_child_output(self):
        self.config()
        source = self.root / 'native-source'
        (source / 'src-tauri').mkdir(parents=True)
        (source / 'src-tauri/tauri.conf.json').write_text('{"version":"1.1.0"}')
        (source / 'src-tauri/Cargo.toml').write_text('[package]\nname="app"\nversion="1.1.0"\n')
        for argv in (['init', '-q'], ['add', '.'], ['-c', 'user.name=Fixture', '-c', 'user.email=fixture@invalid', 'commit', '-qm', 'fixture']):
            subprocess.run(['git', '-C', str(source)] + argv, check=True, capture_output=True)
        self.settings['source_root'] = str(source)
        (self.root / 'missing-private.key').write_text('disposable-not-used-because-build-fails')
        self.settings['oss'] = {field: '' for field in self.settings['oss']}
        self.write_config()
        marker = self.root / 'native-invoked'
        adapter = self.root / 'failed-native.py'
        adapter.write_text('from pathlib import Path\nimport sys\nPath(' + repr(str(marker)) + ').touch()\nprint("private-native-child-sentinel")\nsys.exit(1)\n')
        args = ('--target', 'darwin-aarch64', '--fixture-native-command', sys.executable, str(adapter))
        result = self.local('build', *args)
        self.assertNotEqual(result.returncode, 0)
        self.assertIn('Native build/sign recipe failed', result.stderr)
        self.assertNotIn('private-native-child-sentinel', result.stdout + result.stderr)
        self.assertTrue(marker.exists())
        self.assertFalse((self.root / 'outputs/1.1.0/darwin-aarch64').exists())
        marker.unlink()
        with (source / 'src-tauri/tauri.conf.json').open('a') as stream: stream.write('\n')
        result = self.local('build', *args)
        self.assertNotEqual(result.returncode, 0)
        self.assertIn('clean source_root', result.stderr)
        self.assertFalse(marker.exists())
        self.assertFalse((self.root / 'outputs/1.1.0/darwin-aarch64').exists())

    def test_local_config_reports_fields_and_rejects_git_and_symlink_paths_without_values(self):
        self.config()
        self.settings['oss']['profile'] = ''
        self.write_config()
        result = self.local('candidate', '--inventory', str(self.inventory))
        self.assertNotEqual(result.returncode, 0)
        self.assertIn('oss.profile', result.stderr)
        self.assertNotIn('isolated-bucket', result.stdout + result.stderr)
        for field in ('state_dir', 'output_dir', 'public_key_file', 'private_key_file'):
            with self.subTest(field=field):
                self.config()
                link = self.root / ('link-' + field)
                link.symlink_to(distribution.ROOT, target_is_directory=True)
                self.settings[field] = str(link / 'private-sentinel')
                self.write_config()
                result = self.local('candidate', '--inventory', str(self.inventory))
                self.assertNotEqual(result.returncode, 0)
                self.assertIn(field, result.stderr)
                self.assertIn('outside Git', result.stderr)
                self.assertNotIn('private-sentinel', result.stderr)


if __name__ == '__main__':
    unittest.main()
