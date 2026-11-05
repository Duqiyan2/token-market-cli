"""Reset boundaries using a temporary home; never accesses a real keychain."""
import hashlib
import importlib.util
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch

spec = importlib.util.spec_from_file_location("macos_reset", Path(__file__).resolve().parents[1] / "scripts/reset-macos-client-state.py")
reset = importlib.util.module_from_spec(spec)
spec.loader.exec_module(reset)


class ResetBoundaries(unittest.TestCase):
    def test_session_only_preserves_tools_master_key_and_other_environment(self):
        with tempfile.TemporaryDirectory() as directory:
            home = Path(directory)
            origin = "https://console.example"
            scope = hashlib.sha256(origin.encode()).hexdigest()
            pending = home / ".tokenmarket/client-login" / scope / "pending-login.json"
            pending.parent.mkdir(parents=True)
            pending.write_text("fixture")
            other = pending.parent.parent / "another-environment.json"
            other.write_text("fixture")
            with patch.object(reset, "remove_item") as remove:
                reset.reset_local(home, "fixture.keychain", origin + "/")
                remove.assert_called_once_with("fixture.keychain", reset.SESSION, "platform-" + scope)
            self.assertFalse(pending.exists())
            self.assertTrue(other.exists())

    def test_full_reset_keeps_device_installations_and_agent_configuration(self):
        with tempfile.TemporaryDirectory() as directory:
            home = Path(directory)
            root = home / ".tokenmarket"
            for name in ("desktop-connections/backups", "desktop-connections/issuances", "backups", "client-login", "tools"):
                (root / name).mkdir(parents=True, exist_ok=True)
                (root / name / "fixture").write_text("fixture")
            device = root / "desktop-connections/device-id"
            device.write_text("stable-device")
            (root / "desktop-connections/keys.json").write_text("fixture")
            agent = home / ".agent-config"
            agent.write_text("user config")
            with patch.object(reset, "remove_item", return_value=False) as remove:
                reset.reset_local(home, "fixture.keychain", "https://console.example", True)
                self.assertEqual([call.args[1] for call in remove.call_args_list], [reset.SESSION, reset.TOOL_KEYS])
            self.assertEqual(list(device.parent.iterdir()), [device])
            self.assertEqual(device.read_text(), "stable-device")
            self.assertFalse((root / "backups").exists())
            self.assertFalse((root / "client-login").exists())
            self.assertTrue((root / "tools/fixture").exists())
            self.assertEqual(agent.read_text(), "user config")

    def test_redirected_directory_is_rejected_before_any_keychain_delete(self):
        with tempfile.TemporaryDirectory() as directory:
            home = Path(directory)
            (home / ".tokenmarket").symlink_to(home / "other")
            with patch.object(reset, "remove_item") as remove:
                with self.assertRaisesRegex(RuntimeError, "符号链接"):
                    reset.reset_local(home, "fixture.keychain", "https://console.example", True)
                remove.assert_not_called()


if __name__ == "__main__":
    unittest.main()
