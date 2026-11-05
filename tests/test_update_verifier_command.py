"""Release verifier executable-path regression at the public publish command."""
from __future__ import annotations
import os
import sys
from pathlib import Path
import unittest
import test_update_distribution as distribution


class VerifierCommandPath(unittest.TestCase):
    def test_publisher_rejects_non_utf8_verifier_json_before_writing(self):
        fixture = distribution.DistributionCommand()
        fixture.setUp()
        self.addCleanup(fixture.doCleanups)
        verifier = fixture.root / 'invalid-verifier'
        verifier.write_text('#!' + sys.executable + '\nimport sys\n'
            'sys.stdout.buffer.write(b\'{"verified":true,"verifier":"tauri-plugin-updater/2.14.1","ignored":"\\xff"}\')\n',
            encoding='utf-8')
        verifier.chmod(0o700)
        with fixture.served_store() as (store, base):
            result = fixture.command('publish', '--inventory', str(fixture.inventory), '--fixture',
                '--local-store', str(store), '--verifier', str(verifier),
                env={'TOKEN_MARKET_UPDATE_PUBLIC_BASE_URL': base})
            self.assertNotEqual(result.returncode, 0)
            self.assertIn('Updater signature/version verifier failed', result.stderr)
            self.assertFalse((store / 'releases').exists())
            self.assertFalse((store / 'test/candidate/stable.json').exists())

    def test_publisher_launches_the_real_verifier_when_configured_through_a_symlink(self):
        fixture = distribution.DistributionCommand()
        fixture.setUp()
        self.addCleanup(fixture.doCleanups)
        target = Path(os.environ.get('CARGO_TARGET_DIR', distribution.ROOT / 'src-tauri/target')).resolve()
        verifier = target / 'debug/examples' / ('verify-update-artifacts.exe' if os.name == 'nt' else 'verify-update-artifacts')
        self.assertTrue(verifier.is_file(), 'Build the official verifier example before this release-host check')
        link = fixture.root / verifier.name
        try:
            link.symlink_to(verifier)
        except OSError as error:
            self.skipTest('Host does not permit an executable symlink: ' + str(error))
        with fixture.served_store() as (store, base):
            result = fixture.command('publish', '--inventory', str(fixture.inventory), '--fixture',
                '--local-store', str(store), '--verifier', str(link),
                env={'TOKEN_MARKET_UPDATE_PUBLIC_BASE_URL': base})
            self.assertEqual(result.returncode, 0, result.stderr)
            self.assertTrue((store / 'test/candidate/stable.json').is_file())


if __name__ == '__main__':
    unittest.main()
