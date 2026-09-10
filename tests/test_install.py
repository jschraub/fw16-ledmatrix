"""Exercise provider/plugin installation in a temporary home with host actions stubbed."""

import os
from pathlib import Path
import subprocess
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[1]


class TestInstall(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.home = Path(self.temp.name)
        bins = self.home / "bin"
        bins.mkdir()
        # No root changes, live service changes, or host configuration writes.
        for name, code in (("sudo", 0), ("systemctl", 1)):
            path = bins / name
            path.write_text(f"#!/bin/sh\nexit {code}\n")
            path.chmod(0o755)
        self.env = {**os.environ, "HOME": str(self.home),
                    "XDG_CONFIG_HOME": str(self.home / "config"),
                    "CLAUDE_CONFIG_DIR": str(self.home / "claude"),
                    "PATH": f"{bins}:/usr/bin:/bin"}
        self.unit = self.home / "config/systemd/user/matrixd.service"
        self.plugin = self.home / "config/opencode/plugins/matrix-session.js"

    def run_install(self, *args):
        return subprocess.run(["bash", str(ROOT / "install.sh"), *args],
                              env=self.env, capture_output=True, text=True, timeout=10)

    def test_default_explicit_selection_switch_back_and_uninstall(self):
        for args, provider in (([], "claude"), (["--provider", "opencode-openai"], "opencode-openai"),
                               (["--provider=claude"], "claude")):
            result = self.run_install(*args)
            self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
            unit = self.unit.read_text()
            self.assertIn(f"ExecStart=/usr/bin/env python3 -m matrixd --provider {provider}\n", unit)
            self.assertNotIn("@PROVIDER@", unit)
            self.assertIn(f"WorkingDirectory={ROOT}", unit)
            if provider == "opencode-openai":
                self.assertEqual(self.plugin.read_bytes(), (ROOT / "integration/opencode/matrix-session.js").read_bytes())
        self.assertTrue(self.plugin.exists(), "switching provider does not uninstall integration")
        result = self.run_install("--uninstall")
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        self.assertFalse(self.unit.exists())
        self.assertFalse(self.plugin.exists())

    def test_optional_plugin_and_dry_run(self):
        result = self.run_install("--provider", "opencode-openai", "--dry-run")
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        self.assertIn("--provider opencode-openai", result.stdout)
        self.assertIn(str(self.plugin), result.stdout)
        self.assertFalse(self.unit.exists())
        self.assertFalse(self.plugin.exists())
        result = self.run_install("--provider", "opencode-openai", "--no-opencode")
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        self.assertFalse(self.plugin.exists())
        self.assertIn("--provider opencode-openai", self.unit.read_text())

    def test_invalid_and_missing_provider_fail_before_installation(self):
        for args in (("--provider",), ("--provider", "openai"), ("--provider=unknown",)):
            result = self.run_install(*args)
            self.assertNotEqual(result.returncode, 0)
            self.assertFalse(self.unit.exists())
            self.assertFalse(self.plugin.exists())
