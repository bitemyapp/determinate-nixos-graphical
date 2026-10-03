#!/usr/bin/env python3
"""Boot the actual hybrid ISO as USB media; optionally install and reboot.

Runs as the current user. The only writable disk is a fresh regular file in
.work; no host block device is ever attached. Control uses QEMU's guest agent,
already enabled by the official graphical installer, and local Unix sockets.
"""

import argparse
import base64
import hashlib
import json
import os
from pathlib import Path
import shutil
import socket
import subprocess
import time

REPO = Path(__file__).resolve().parents[1]


class RPC:
    def __init__(self, path, qmp=False):
        self.sock = socket.socket(socket.AF_UNIX)
        self.sock.settimeout(20)
        self.sock.connect(str(path))
        self.stream = self.sock.makefile("rwb", buffering=0)
        if qmp:
            self.stream.readline()
            self.call("qmp_capabilities")

    def call(self, command, arguments=None):
        request = {"execute": command, "id": time.monotonic_ns()}
        if arguments is not None:
            request["arguments"] = arguments
        self.stream.write(json.dumps(request).encode() + b"\n")
        while True:
            response = json.loads(self.stream.readline())
            if response.get("id") != request["id"]:
                continue
            if "error" in response:
                raise RuntimeError(response["error"])
            if "return" in response:
                return response["return"]

    def close(self):
        self.stream.close()
        self.sock.close()


class VM:
    def __init__(self, work, firmware, iso=None, disk=None, share=False):
        self.work = work
        self.work.mkdir(parents=True, exist_ok=True)
        self.qga_path = work / "qga.sock"
        self.qmp_path = work / "qmp.sock"
        for path in (self.qga_path, self.qmp_path):
            if path.exists():
                raise RuntimeError(f"Socket already exists: {path}; is a VM still running?")
        args = ["qemu-system-x86_64", "-name", "determinate-respin-test",
                "-machine", "q35,accel=kvm", "-cpu", "host", "-smp", "4", "-m", "8192",
                "-display", "none", "-vga", "std", "-no-reboot",
                "-serial", f"file:{work / 'serial.log'}",
                "-qmp", f"unix:{self.qmp_path},server=on,wait=off",
                "-device", "virtio-serial-pci",
                "-chardev", f"socket,path={self.qga_path},server=on,wait=off,id=qga",
                "-device", "virtserialport,chardev=qga,name=org.qemu.guest_agent.0",
                "-netdev", "user,id=net0", "-device", "virtio-net-pci,netdev=net0"]
        if firmware == "uefi":
            code = Path(os.environ.get("OVMF_CODE", "/usr/share/edk2/x64/OVMF_CODE.4m.fd"))
            original_vars = Path(os.environ.get("OVMF_VARS", "/usr/share/edk2/x64/OVMF_VARS.4m.fd"))
            variables = work / "OVMF_VARS.fd"
            if not variables.exists():
                shutil.copyfile(original_vars, variables)
            args += ["-drive", f"if=pflash,format=raw,readonly=on,file={code}",
                     "-drive", f"if=pflash,format=raw,file={variables}"]
        if iso:
            args += ["-device", "qemu-xhci,id=xhci",
                     "-drive", f"if=none,id=iso,format=raw,readonly=on,file={iso}",
                     "-device", "usb-storage,drive=iso,bootindex=1"]
        if disk:
            args += ["-drive", f"if=none,id=target,format=raw,file={disk}",
                     "-device", "virtio-blk-pci,drive=target,serial=RESPIN_TEST_ONLY,bootindex=2"]
        if share:
            args += ["-virtfs", f"local,path={REPO},mount_tag=project,security_model=none,id=project"]
        self.log = (work / "qemu.log").open("wb")
        self.process = subprocess.Popen(args, stdin=subprocess.DEVNULL, stdout=self.log, stderr=self.log)
        self.agent = None

    def wait_agent(self, timeout=240):
        deadline = time.monotonic() + timeout
        while time.monotonic() < deadline:
            if self.process.poll() is not None:
                raise RuntimeError(f"QEMU exited: {(self.work / 'qemu.log').read_text()}")
            try:
                agent = RPC(self.qga_path)
                agent.sock.settimeout(3)
                agent.call("guest-ping")
                agent.sock.settimeout(20)
                self.agent = agent
                return
            except (OSError, ValueError, RuntimeError):
                if 'agent' in locals():
                    agent.close()
                time.sleep(2)
        raise TimeoutError(f"Guest agent not ready; see {self.work / 'serial.log'}")

    def execute(self, command, timeout=120):
        result = self.agent.call("guest-exec", {
            "path": "/run/current-system/sw/bin/bash", "arg": ["-lc", command], "capture-output": True,
        })
        deadline = time.monotonic() + timeout
        while time.monotonic() < deadline:
            status = self.agent.call("guest-exec-status", {"pid": result["pid"]})
            if status.get("exited"):
                output = "".join(base64.b64decode(status.get(key, "")).decode(errors="replace")
                                 for key in ("out-data", "err-data"))
                if status.get("exitcode", 1) != 0:
                    raise RuntimeError(f"Guest command failed: {command}\n{output}")
                return output
            time.sleep(1)
        raise TimeoutError(f"Guest command timed out: {command}")

    def screenshot(self, destination):
        monitor = RPC(self.qmp_path, qmp=True)
        try:
            monitor.call("screendump", {"filename": str(destination), "format": "png"})
        finally:
            monitor.close()

    def stop(self):
        if self.agent:
            self.agent.close()
        if self.process.poll() is None:
            self.process.terminate()
            try:
                self.process.wait(timeout=15)
            except subprocess.TimeoutExpired:
                self.process.kill()
                self.process.wait()
        self.log.close()
        for path in (self.qga_path, self.qmp_path):
            path.unlink(missing_ok=True)


def verify_installed(vm, artifacts):
    vm.wait_agent()
    vm.execute("for i in $(seq 1 90); do systemctl is-active --quiet display-manager && exit 0; sleep 1; done; exit 1")
    output = vm.execute(
        "set -e; test \"$(hostname)\" = respin-test; nix --version; fh --version; "
        "systemctl is-active determinate-nixd.socket; "
        "systemctl is-active display-manager; "
        "test ! -e /etc/determinate-installer; "
        "test ! -e /run/current-system/sw/bin/calamares; "
        "id respintest; ! id nixos; sha256sum /etc/nixos/flake.lock")
    if "nix (Determinate Nix " not in output:
        raise RuntimeError(f"Installed Nix is not Determinate Nix:\n{output}")
    expected_lock = hashlib.sha256((REPO / "flake.lock").read_bytes()).hexdigest()
    if expected_lock not in output:
        raise RuntimeError(f"Installed input lock differs from the repository:\n{output}")
    time.sleep(10)
    vm.screenshot(artifacts / "installed-desktop.png")
    return output


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("iso", type=Path)
    parser.add_argument("--firmware", choices=["bios", "uefi"], default="uefi")
    parser.add_argument("--install", action="store_true", help="Exercise real Calamares backend, install Plasma, reboot")
    args = parser.parse_args()
    iso = args.iso.resolve(strict=True)
    if not iso.is_file():
        parser.error("ISO must be a regular file, never a block device")
    with iso.open("rb") as source:
        iso_sha256 = hashlib.file_digest(source, "sha256").hexdigest()
    stamp = time.strftime("%Y%m%d-%H%M%S")
    name = f"{args.firmware}-{stamp}"
    work = REPO / ".work" / name
    work.mkdir(parents=True)
    artifacts = REPO / "artifacts" / name
    artifacts.mkdir(parents=True)
    disk = None
    if args.install:
        disk = work / "target.raw"
        with disk.open("xb") as output:
            output.truncate(40 * 1024**3)
    result = {"firmware": args.firmware, "media": "usb-storage", "iso": iso.name,
              "iso_sha256": iso_sha256, "install": args.install, "passed": False}
    vm = VM(work, args.firmware, iso=iso, disk=disk, share=args.install)
    try:
        print(f"Booting {name} from the ISO as a USB mass-storage device", flush=True)
        vm.wait_agent()
        # Both the desktop and installer must actually be running.
        deadline = time.monotonic() + 180
        while True:
            try:
                result["live"] = vm.execute(
                    "set -e; systemctl is-active display-manager; pgrep -f '^/[^ ]+/bin/[.]?plasmashell'; "
                    "pgrep -f '^/[^ ]+/bin/[.]?calamares'; nix --version; fh --version; "
                    "systemctl is-active determinate-nixd.socket; "
                    "test -s /etc/determinate-installer/flake.lock; "
                    "lsblk -o NAME,TRAN,FSTYPE,LABEL")
                break
            except RuntimeError:
                if time.monotonic() > deadline:
                    raise
                time.sleep(3)
        time.sleep(5)
        vm.screenshot(artifacts / "live-desktop.png")
        print(result["live"], flush=True)
        if args.install:
            vm.execute("mkdir -p /workspace; mount -t 9p -o trans=virtio,version=9p2000.L project /workspace")
            print("Installing to the disposable 40 GiB virtual disk via the real Calamares module", flush=True)
            log = f"/workspace/artifacts/{name}/install.log"
            vm.execute(f"set -e; python=$(command -v python3 || ls /nix/store/*-python3-*/bin/python3 | head -1); "
                       f"\"$python\" /workspace/tests/install_in_guest.py > {log} 2>&1", timeout=3600)
            result["target_before_reboot"] = vm.execute(
                "set -e; cat /mnt/etc/nixos/flake.nix; sha256sum /mnt/etc/nixos/flake.lock; "
                # Absolute /nix/store symlinks must be resolved inside the
                # target, not by the live system. Verify Nix after booting it.
                "test -L /mnt/nix/var/nix/profiles/system; "
                "readlink /mnt/nix/var/nix/profiles/system; sync")
            vm.execute("umount -R /mnt; sync")
            vm.agent.call("guest-shutdown", {"mode": "powerdown"})
            vm.process.wait(timeout=60)
            vm.stop()
            shutil.copyfile(work / "serial.log", artifacts / "live-serial.log")
            vm = VM(work, args.firmware, disk=disk)
            print("Booting the installed disk with the installer ISO removed", flush=True)
            result["installed"] = verify_installed(vm, artifacts)
            print(result["installed"], flush=True)
        result["passed"] = True
    finally:
        if not result["passed"] and vm.process.poll() is None:
            try:
                vm.screenshot(artifacts / "failure.png")
                if vm.agent:
                    (artifacts / "diagnostics.log").write_text(vm.execute(
                        "systemctl --failed; ps -eo uid,pid,comm,args; journalctl -b -p warning --no-pager | tail -150"))
            except (OSError, RuntimeError, TimeoutError):
                pass
        vm.stop()
        for filename in ("serial.log", "qemu.log"):
            if (work / filename).exists():
                shutil.copyfile(work / filename, artifacts / filename)
        (artifacts / "result.json").write_text(json.dumps(result, indent=2) + "\n")
    print(f"PASS: {artifacts}", flush=True)


if __name__ == "__main__":
    main()
