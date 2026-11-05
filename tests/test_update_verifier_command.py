"""Release verifier executable-path regression at the public publish command."""
from __future__ import annotations
import os
from pathlib import Path
import unittest
import test_update_distribution as distribution


class VerifierCommandPath(unittest.TestCase):
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
