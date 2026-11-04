"""Release guards; no real credentials, signing identities, or keychain writes."""
import argparse
import base64
import importlib.util
import os
from pathlib import Path
import plistlib
import tempfile
import unittest
from unittest.mock import patch

SCRIPT = Path(__file__).resolve().parents[1] / "scripts/macos-local-signing.py"
SPEC = importlib.util.spec_from_file_location("macos_signing", SCRIPT)
signing = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(signing)


class SigningGuards(unittest.TestCase):
    def args(self, **values):
        return argparse.Namespace(ci=True, profile="production", **values)

    def test_missing_ci_secrets_cannot_generate_or_sign_an_identity(self):
        with patch.dict(os.environ, {}, clear=True), patch.object(signing, "run") as run:
            with self.assertRaisesRegex(RuntimeError, "ad-hoc fallback is disabled"):
                with signing.signing_identity(self.args()):
                    self.fail("missing release secrets were accepted")
            run.assert_not_called()

    def test_invalid_certificate_is_rejected_without_revealing_it(self):
        secret = "not-a-base64-certificate!"
        with patch.dict(os.environ, {
            "MACOS_SIGNING_CERTIFICATE": secret,
            "MACOS_SIGNING_PASSWORD": "fixture-password",
            "MACOS_SIGNING_CERTIFICATE_SHA1": "A" * 40,
        }), patch.object(signing, "run") as run:
            with self.assertRaises(RuntimeError) as error:
                with signing.signing_identity(self.args()):
                    self.fail("invalid certificate was accepted")
            self.assertNotIn(secret, str(error.exception))
            run.assert_not_called()

    def test_changed_ci_certificate_cannot_change_the_release_identity(self):
        with patch.dict(os.environ, {
            "MACOS_SIGNING_CERTIFICATE": base64.b64encode(b"fixture archive").decode(),
            "MACOS_SIGNING_PASSWORD": "fixture-password",
            "MACOS_SIGNING_CERTIFICATE_SHA1": "A" * 40,
        }), patch.object(signing, "run", return_value=b"fixture certificate") as run, \
                patch.object(signing, "fingerprint", return_value="B" * 40):
            with self.assertRaisesRegex(RuntimeError, "certificate changed"):
                with signing.signing_identity(self.args()):
                    self.fail("changed certificate was accepted")
            # Rejection precedes any keychain operation, including importing keys.
            self.assertTrue(all(call.args[0][0] == "/usr/bin/openssl" for call in run.call_args_list))

    def test_missing_local_identity_is_not_silently_replaced(self):
        with tempfile.TemporaryDirectory() as directory, patch.object(signing, "run") as run:
            with self.assertRaisesRegex(RuntimeError, "identity missing"):
                with signing.identity(Path(directory)):
                    self.fail("a replacement signing identity was created")
            run.assert_not_called()

    def test_production_package_rejects_a_local_app_before_copying_it(self):
        with tempfile.TemporaryDirectory() as directory:
            app = Path(directory) / "Token Market Local.app"
            (app / "Contents").mkdir(parents=True)
            with (app / "Contents/Info.plist").open("wb") as stream:
                plistlib.dump({"CFBundleIdentifier": signing.APP_ID}, stream)
            args = self.args(app=app, cli=Path(directory) / "cli", output=Path(directory) / "out")
            with patch.object(signing, "run") as run:
                with self.assertRaisesRegex(RuntimeError, "signing profile"):
                    signing.package(args)
                run.assert_not_called()
            self.assertFalse(args.output.exists())


if __name__ == "__main__":
    unittest.main()
