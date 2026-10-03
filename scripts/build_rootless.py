#!/usr/bin/env python3
"""Build using the existing Determinate ISO in an unprivileged QEMU/KVM VM.

Requires Python 3, QEMU x86, bsdtar, OpenSSH and read/write access to /dev/kvm.
No host Nix installation, sudo, containers, loop mounts or USB access required.
"""

import argparse
import hashlib
import os
from pathlib import Path
import re
import socket
import subprocess
import threading
import time

REPO = Path(__file__).resolve().parents[1]
BOOTSTRAP_SHA256 = "80588c226d84e16fe11b2e4afa9fc4add02902e7041dcb220960df5a6cde5fb5"


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("bootstrap_iso", type=Path)
    parser.add_argument("--sha256", default=BOOTSTRAP_SHA256, help="Expected bootstrap ISO SHA-256")
    args = parser.parse_args()
    iso = args.bootstrap_iso.resolve(strict=True)
    if not iso.is_file():
        parser.error("The bootstrap ISO must be a regular file")
    with iso.open("rb") as source:
        digest = hashlib.file_digest(source, "sha256").hexdigest()
    if digest != args.sha256:
        parser.error(f"Bootstrap SHA-256 mismatch: {digest}")
    if not os.access("/dev/kvm", os.R_OK | os.W_OK):
        parser.error("KVM is not accessible to this user; configure KVM access first")
    work = REPO / ".work/rootless"
    work.mkdir(parents=True, exist_ok=True)
    serial_path = work / "serial.sock"
    if serial_path.exists():
        parser.error(f"{serial_path} already exists; check for a running builder")
    key = work / "ssh-key"
    if not key.exists():
        subprocess.run(["ssh-keygen", "-q", "-t", "ed25519", "-N", "", "-C", "respin-builder", "-f", str(key)], check=True)
    disk = work / "builder.raw"
    if not disk.exists():
        with disk.open("xb") as output:
            output.truncate(100 * 1024**3)
    if not disk.is_file() or disk.is_symlink():
        parser.error("The builder disk must be a regular file, not a device or symlink")
    cfg = subprocess.check_output(["bsdtar", "-xOf", str(iso), "isolinux/isolinux.cfg"], text=True)
    entry = cfg.split("LABEL boot\n", 1)[1].split("\nLABEL ", 1)[0]
    kernel = re.search(r"^LINUX (.+)$", entry, re.M)[1].lstrip("/").replace("//", "/")
    initrd = re.search(r"^INITRD (.+)$", entry, re.M)[1].lstrip("/").replace("//", "/")
    append = re.search(r"^APPEND (.+)$", entry, re.M)[1] + " console=ttyS0,115200n8"
    subprocess.run(["bsdtar", "-xf", str(iso), "-C", str(work), kernel, initrd], check=True)
    with socket.socket() as reserve:
        reserve.bind(("127.0.0.1", 0))
        port = reserve.getsockname()[1]
    qemu_args = [
        "qemu-system-x86_64", "-name", "determinate-respin-builder",
        "-machine", "q35,accel=kvm", "-cpu", "host", "-smp", "6", "-m", "12288",
        "-display", "none", "-monitor", "none", "-no-reboot",
        "-serial", f"unix:{serial_path},server=on,wait=off",
        "-kernel", str(work / kernel), "-initrd", str(work / initrd), "-append", append,
        "-drive", f"file={iso},format=raw,media=cdrom,readonly=on",
        "-drive", f"file={disk},format=raw,if=none,id=builder",
        "-device", "virtio-blk-pci,drive=builder,serial=RESPIN_BUILDER_ONLY",
        "-netdev", f"user,id=net0,hostfwd=tcp:127.0.0.1:{port}-:22",
        "-device", "virtio-net-pci,netdev=net0",
        "-virtfs", f"local,path={REPO},mount_tag=project,security_model=none,id=project",
    ]
    artifacts = REPO / "artifacts"
    artifacts.mkdir(exist_ok=True)
    with (work / "qemu.log").open("wb") as qemu_log:
        print("Starting the builder VM; boot/setup log: .work/rootless/serial.log", flush=True)
        qemu = subprocess.Popen(qemu_args, stdin=subprocess.DEVNULL, stdout=qemu_log, stderr=qemu_log)
        serial = socket.socket(socket.AF_UNIX)
        stop_reader = threading.Event()
        reader = None
        try:
            deadline = time.monotonic() + 600
            while not serial_path.exists():
                if qemu.poll() is not None or time.monotonic() > deadline:
                    raise RuntimeError("QEMU did not start; inspect .work/rootless/qemu.log")
                time.sleep(0.2)
            serial.connect(str(serial_path))
            serial.settimeout(1)
            received = b""
            sent = False
            with (work / "serial.log").open("wb") as log:
                while time.monotonic() < deadline:
                    try:
                        data = serial.recv(65536)
                    except socket.timeout:
                        continue
                    if not data:
                        raise RuntimeError("Builder serial console closed")
                    log.write(data)
                    log.flush()
                    received += data
                    if not sent and b"nixos@nixos:" in received:
                        serial.sendall(b"sudo mkdir -p /workspace; sudo mount -t 9p -o trans=virtio,version=9p2000.L project /workspace; sudo bash /workspace/scripts/prepare-builder.sh\n")
                        sent = True
                        received = b""
                    if sent and b"RESPIN_BUILDER_READY" in received:
                        break
                else:
                    raise TimeoutError("Builder setup timed out; inspect .work/rootless/serial.log")
            # Keep consuming the serial console after bootstrap. An unread
            # socket can block the guest while systemd prints shutdown output.
            def drain_console():
                with (work / "serial.log").open("ab", buffering=0) as log:
                    while not stop_reader.is_set():
                        try:
                            data = serial.recv(65536)
                            if not data:
                                break
                            log.write(data)
                        except socket.timeout:
                            continue
                        except OSError:
                            break
            reader = threading.Thread(target=drain_console, daemon=True)
            reader.start()
            command = (
                "set -e; cd /workspace; export TMPDIR=/build/tmp; "
                "nix flake check --no-update-lock-file -L; "
                "nix build .#iso --out-link /build/result --no-update-lock-file --cores 6 --max-jobs 2 -L; "
                "install -m 644 /build/result/iso/*.iso -t /workspace/artifacts/; "
                "cd /workspace/artifacts; "
                "for image in *.iso; do sha256sum \"$image\" | tee \"$image.sha256\"; done; sync"
            )
            ssh = ["ssh", "-i", str(key), "-p", str(port), "-o", "BatchMode=yes", "-o", "ConnectTimeout=15",
                   "-o", "StrictHostKeyChecking=accept-new",
                   "-o", f"UserKnownHostsFile={work / 'known-hosts'}", "root@127.0.0.1"]
            print("Building in QEMU; full output: artifacts/build.log", flush=True)
            with (artifacts / "build.log").open("wb") as log:
                subprocess.run(ssh + [command], stdout=log, stderr=subprocess.STDOUT, check=True)
            subprocess.run(ssh + ["poweroff"], check=False, timeout=20)
            qemu.wait(timeout=45)
        finally:
            stop_reader.set()
            serial.close()
            if reader:
                reader.join(timeout=2)
            if qemu.poll() is None:
                qemu.terminate()
                try:
                    qemu.wait(timeout=15)
                except subprocess.TimeoutExpired:
                    qemu.kill()
                    qemu.wait()
            serial_path.unlink(missing_ok=True)
    print(f"Build complete: {artifacts}")


if __name__ == "__main__":
    main()
