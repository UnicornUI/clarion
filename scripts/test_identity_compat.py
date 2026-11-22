#!/usr/bin/env python3
"""Identity migration uses one executable and preserves literal CLI arguments and user data."""
import json
import importlib.machinery
import importlib.util
import socket
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile
import unittest


ROOT = Path(__file__).resolve().parent.parent

loader = importlib.machinery.SourceFileLoader("clarion_shell_msg", str(ROOT / "resources/clarion-shell-msg"))
shell_msg_spec = importlib.util.spec_from_loader(loader.name, loader)
shell_msg = importlib.util.module_from_spec(shell_msg_spec)
loader.exec_module(shell_msg)


class IdentityCompatibilityTests(unittest.TestCase):
    def test_legacy_commands_forward_literal_arguments_to_the_sibling_binary(self):
        with tempfile.TemporaryDirectory(prefix="clarion wrappers ") as temporary:
            root = Path(temporary)
            for suffix, canonical_name in (
                ("", "clarion"),
                ("-session", "clarion-session"),
                ("-settings", "clarion-settings"),
                ("-shell", "clarion-shell"),
                ("-shell-msg", "clarion-shell-msg"),
            ):
                legacy = root / ("realand" + suffix)
                canonical = root / canonical_name
                shutil.copy2(ROOT / "resources/compat" / legacy.name, legacy)
                canonical.write_text(
                    "#!/usr/bin/env python3\nimport json, sys\n"
                    "print(json.dumps(sys.argv))\n"
                )
                canonical.chmod(0o755)
                arguments = ["spaces stay together", "$(touch should-not-exist)", "\"'`", "line\nbreak"]
                result = subprocess.run(
                    [str(legacy), *arguments], check=True, text=True,
                    stdout=subprocess.PIPE, stderr=subprocess.PIPE,
                    cwd=root, timeout=5,
                )
                self.assertEqual(json.loads(result.stdout), [str(canonical), *arguments])
                self.assertFalse((root / "should-not-exist").exists())

    def test_settings_install_removes_the_retired_launcher(self):
        with tempfile.TemporaryDirectory(prefix="clarion settings install ") as temporary:
            root = Path(temporary)
            binary = root / "built-settings"
            binary.write_text("#!/bin/sh\nexit 0\n")
            binary.chmod(0o755)
            prefix = root / "local"
            old = prefix / "bin/clarion-system-settings"
            old.parent.mkdir(parents=True)
            old.write_text("interim launcher\n")
            old_desktop = prefix / "share/applications/clarion-system-settings.desktop"
            old_desktop.parent.mkdir(parents=True)
            old_desktop.write_text("[Desktop Entry]\n")
            retired_root = prefix / "lib/clarion-system-settings/releases/old"
            retired_root.mkdir(parents=True)
            (retired_root / "clarion-system-settings").write_text("interim binary\n")
            result = subprocess.run(
                [sys.executable, str(ROOT / "scripts/install-settings.py"),
                 "--binary", str(binary), "--prefix", str(prefix)],
                check=True, text=True, stdout=subprocess.PIPE, stderr=subprocess.PIPE, timeout=10,
            )
            receipt = json.loads(result.stdout)
            launcher = prefix / "bin/clarion-settings"
            self.assertTrue(launcher.is_symlink())
            self.assertEqual(launcher.read_bytes(), binary.read_bytes())
            self.assertFalse(old.exists())
            self.assertFalse(old_desktop.exists())
            self.assertIn(str(old), receipt["removed_legacy"])
            self.assertIn(str(old_desktop), receipt["removed_legacy"])
            self.assertEqual(receipt["removed_legacy_root"], str(prefix / "lib/clarion-system-settings"))
            self.assertFalse((prefix / "lib/clarion-system-settings").exists())
            desktop = prefix / "share/applications/clarion-settings.desktop"
            self.assertIn("clarion-settings", desktop.read_text())

    def test_shell_install_preserves_alias_and_does_not_start_a_service(self):
        with tempfile.TemporaryDirectory(prefix="clarion shell install ") as temporary:
            root = Path(temporary)
            binary = root / "clarion-shell"
            binary.write_bytes(b"native shell fixture")
            binary.chmod(0o755)
            assets = root / "icons"
            assets.mkdir()
            (assets / "LICENSE").write_text("test open license\n")
            (assets / "app.svg").write_text('<svg xmlns="http://www.w3.org/2000/svg"/>')
            prefix = root / "local"
            result = subprocess.run(
                [sys.executable, str(ROOT / "scripts/install-shell.py"),
                 "--binary", str(binary), "--assets", str(assets), "--prefix", str(prefix),
                 "--config-root", str(root / "config")],
                check=True, text=True, stdout=subprocess.PIPE, stderr=subprocess.PIPE, timeout=10,
            )
            receipt = json.loads(result.stdout)
            self.assertFalse(receipt["service_enabled_or_started"])
            self.assertFalse(receipt["existing_shell_changed"])
            self.assertEqual((prefix / "bin/realand-shell").resolve(),
                             (prefix / "bin/clarion-shell").resolve())
            self.assertEqual((prefix / "bin/realand-shell-msg").resolve(),
                             (prefix / "bin/clarion-shell-msg").resolve())
            self.assertFalse((root / "config/systemd/user/graphical-session.target.wants").exists())

    def test_panel_socket_selection_never_retries_an_authoritative_canonical_socket(self):
        environment = {"XDG_RUNTIME_DIR": "/run/user/fixture"}
        canonical = Path("/run/user/fixture/clarion-shell.sock")
        legacy = Path("/run/user/fixture/realand-shell.sock")
        self.assertEqual(shell_msg.socket_path(environment, present=lambda _: True), canonical)
        self.assertEqual(shell_msg.socket_path(environment, present=lambda p: p == legacy), legacy)
        self.assertEqual(shell_msg.socket_path(environment, present=lambda _: False), canonical)
        self.assertEqual(shell_msg.socket_path({"CLARION_SHELL_SOCKET": "/new", "REALAND_SHELL_SOCKET": "/old"}), Path("/new"))
        self.assertEqual(shell_msg.socket_path({"REALAND_SHELL_SOCKET": "/old"}), Path("/old"))
        with self.assertRaises(ValueError):
            shell_msg.socket_path({"XDG_RUNTIME_DIR": "relative"})

    def test_panel_client_sends_one_literal_command_without_starting_a_shell(self):
        with tempfile.TemporaryDirectory(prefix="clarion-msg-") as temporary:
            path = Path(temporary) / "panel.sock"
            with socket.socket(socket.AF_UNIX, socket.SOCK_STREAM) as server:
                server.bind(str(path))
                server.listen(1)
                server.settimeout(1)
                shell_msg.send_command(path, "spotlight")
                client, _ = server.accept()
                with client:
                    client.settimeout(1)
                    self.assertEqual(client.recv(1024), b"spotlight\n")
                    self.assertEqual(client.recv(1), b"")


if __name__ == "__main__":
    unittest.main()
