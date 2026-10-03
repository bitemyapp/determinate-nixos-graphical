"""Seed the target flake before Calamares invokes nixos-install.

Only the live installer calls this module. No credentials, live-session users,
autologin settings, or installer services are added to the target configuration.
"""

import json
from pathlib import Path
import re
import shutil


def prepare_target(root, hostname, template_dir="/etc/determinate-installer"):
    hostname = hostname or "nixos"
    if not isinstance(hostname, str) or not re.fullmatch(
        r"[A-Za-z0-9](?:[A-Za-z0-9-]{0,61}[A-Za-z0-9])?", hostname
    ):
        raise ValueError("The target hostname must be a DNS label of 1 to 63 characters")
    root = Path(root)
    if not root.is_absolute() or root.resolve() == Path("/"):
        raise ValueError("Expected an absolute installation root other than /")
    destination = root / "etc/nixos"
    if not (destination / "configuration.nix").is_file():
        raise ValueError("Calamares must generate configuration.nix first")
    if not (destination / "hardware-configuration.nix").is_file():
        raise ValueError("Hardware configuration is missing")
    template_dir = Path(template_dir)
    template = (template_dir / "flake.nix.in").read_text()
    if template.count("@HOSTNAME@") != 1:
        raise ValueError("Expected exactly one hostname placeholder in the target flake")
    lock = template_dir / "flake.lock"
    lock_data = json.loads(lock.read_text())
    if set(lock_data["nodes"][lock_data["root"]]["inputs"]) != {"nixpkgs", "determinate", "fh"}:
        raise ValueError("Target lock file has unexpected root inputs")
    (destination / "flake.nix").write_text(template.replace("@HOSTNAME@", json.dumps(hostname)))
    shutil.copyfile(lock, destination / "flake.lock")
    return f"{destination}#{hostname}"
