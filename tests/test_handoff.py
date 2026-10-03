#!/usr/bin/env python3
"""Regression tests for the real patched Calamares -> nixos-install boundary."""

import ast
import importlib.util
import json
from pathlib import Path
import shutil
import sys
import tempfile
import unittest

helper_path, template_path, lock_path, patched_main = map(Path, sys.argv[1:5])
sys.argv[1:] = []
spec = importlib.util.spec_from_file_location("prepare_target", helper_path)
helper = importlib.util.module_from_spec(spec)
spec.loader.exec_module(helper)


class HandoffTests(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.addCleanup(self.tmp.cleanup)
        base = Path(self.tmp.name)
        self.root = base / "target"
        self.config = self.root / "etc/nixos"
        self.config.mkdir(parents=True)
        (self.config / "configuration.nix").write_text("{ networking.hostName = \"test-host\"; }")
        (self.config / "hardware-configuration.nix").write_text("{}")
        self.templates = base / "templates"
        self.templates.mkdir()
        shutil.copyfile(template_path, self.templates / "flake.nix.in")
        shutil.copyfile(lock_path, self.templates / "flake.lock")

    def prepare(self, hostname="test-host"):
        return helper.prepare_target(str(self.root), hostname, str(self.templates))

    def test_gui_hostname_and_identical_lock(self):
        original = (self.config / "configuration.nix").read_bytes()
        self.assertEqual(self.prepare(), f"{self.config}#test-host")
        self.assertEqual((self.config / "configuration.nix").read_bytes(), original)
        self.assertEqual((self.config / "flake.lock").read_bytes(), lock_path.read_bytes())
        generated = (self.config / "flake.nix").read_text()
        self.assertIn('nixosConfigurations."test-host"', generated)
        self.assertIn("determinate.nixosModules.default", generated)
        self.assertNotIn("@HOSTNAME@", generated)

    def test_default_hostname(self):
        self.assertTrue(self.prepare(None).endswith("#nixos"))

    def test_invalid_hostname_rejected_before_writing(self):
        for hostname in ['../oops', '${builtins.abort "oops"}', '-bad', 'bad-', 'x' * 64, 123]:
            with self.subTest(hostname=hostname), self.assertRaises(ValueError):
                self.prepare(hostname)
        self.assertFalse((self.config / "flake.nix").exists())

    def test_missing_hardware_rejected(self):
        (self.config / "hardware-configuration.nix").unlink()
        with self.assertRaises(ValueError):
            self.prepare()

    def test_host_root_rejected(self):
        with self.assertRaises(ValueError):
            helper.prepare_target("/", "test-host", str(self.templates))

    def test_real_install_argv_selects_flake(self):
        # Execute the argument-construction statements from the actual patched
        # upstream module, not a duplicate of the implementation in this test.
        tree = ast.parse(patched_main.read_text())
        run = next(node for node in tree.body if isinstance(node, ast.FunctionDef) and node.name == "run")
        start = next(i for i, node in enumerate(run.body) if isinstance(node, ast.Assign)
                     and any(isinstance(t, ast.Name) and t.id == "nixosInstallCmd" for t in node.targets))
        statements = ast.Module(body=run.body[start:start + 3], type_ignores=[])
        context = {"root_mount_point": str(self.root), "target_flake": self.prepare(),
                   "generateProxyStrings": lambda: []}
        exec(compile(statements, str(patched_main), "exec"), context)
        argv = context["nixosInstallCmd"]
        self.assertEqual(argv[argv.index("--root") + 1], str(self.root))
        self.assertEqual(argv[argv.index("--flake") + 1], f"{self.config}#test-host")
        self.assertIn("--no-root-passwd", argv)


unittest.main()
