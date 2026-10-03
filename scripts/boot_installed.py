#!/usr/bin/env python3
"""Resume disk-only verification after a completed Calamares test installation.

Accepts only a run produced by qemu_test.py with a successful installation log.
This does not partition or reinstall anything. It never attaches the ISO.
"""

import argparse
import json
from pathlib import Path
import shutil
import time

from qemu_test import REPO, VM, verify_installed


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("run", help="Run name, e.g. uefi-20261003-175025")
    args = parser.parse_args()
    if Path(args.run).name != args.run or args.run in (".", ".."):
        parser.error("Supply a run name, not a path")
    previous_artifacts = REPO / "artifacts" / args.run
    previous_work = REPO / ".work" / args.run
    result = json.loads((previous_artifacts / "result.json").read_text())
    install_log = (previous_artifacts / "install.log").read_text()
    if not result.get("install") or not install_log.rstrip().endswith("CALAMARES_INSTALL_PASS"):
        parser.error("The run does not contain a successful Calamares installation")
    if result.get("firmware") not in ("bios", "uefi") or "live" not in result or "iso_sha256" not in result:
        parser.error("The original run's firmware/live-media evidence is incomplete")
    disk = previous_work / "target.raw"
    if disk.is_symlink() or not disk.is_file() or disk.stat().st_size != 40 * 1024**3:
        parser.error("Expected the original regular 40 GiB test disk")
    name = f"{args.run}-boot-{time.strftime('%H%M%S')}"
    work = REPO / ".work" / name
    artifacts = REPO / "artifacts" / name
    work.mkdir()
    artifacts.mkdir()
    if result["firmware"] == "uefi":
        shutil.copyfile(previous_work / "OVMF_VARS.fd", work / "OVMF_VARS.fd")
    shutil.copyfile(previous_artifacts / "live-desktop.png", artifacts / "live-desktop.png")
    shutil.copyfile(previous_artifacts / "install.log", artifacts / "install.log")
    result.update(passed=False, resumed_from=args.run, disk_only_boot=True)
    vm = VM(work, result["firmware"], disk=disk)
    try:
        print(f"Booting the installed {result['firmware']} disk; no ISO attached", flush=True)
        result["installed"] = verify_installed(vm, artifacts)
        print(result["installed"], flush=True)
        result["passed"] = True
    finally:
        if not result["passed"] and vm.process.poll() is None:
            try:
                vm.screenshot(artifacts / "failure.png")
            except (OSError, ValueError, RuntimeError):
                pass
        vm.stop()
        for filename in ("serial.log", "qemu.log"):
            if (work / filename).exists():
                shutil.copyfile(work / filename, artifacts / filename)
        (artifacts / "result.json").write_text(json.dumps(result, indent=2) + "\n")
    print(f"PASS: {artifacts}")


if __name__ == "__main__":
    main()
