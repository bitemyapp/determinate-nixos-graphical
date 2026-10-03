#!/usr/bin/env python3
"""Real installation using the packaged Calamares Python job and fixed UI data.

The UI itself is separately smoke-tested. Here only libcalamares's global
storage, progress and logging interfaces are supplied by a fixture. Partition
creation, hardware scanning, Nix evaluation/build, copy and bootloader install
are real. The target gets a guest agent and serial console for reboot checks;
these two diagnostic options are NOT present in normal GUI installations.
"""

import importlib.util
import os
from pathlib import Path
import subprocess
import sys
import types


def run(*args, **kwargs):
    return subprocess.run(args, check=True, text=True, **kwargs)


if os.geteuid() != 0:
    raise RuntimeError("Run only inside the disposable test VM as root")
serial = Path("/sys/class/block/vda/serial")
if not serial.is_file() or serial.read_text().strip() != "RESPIN_TEST_ONLY":
    raise RuntimeError("Refusing to partition a disk without the test-only serial")
if subprocess.check_output(["blockdev", "--getsize64", "/dev/vda"], text=True).strip() != str(40 * 1024**3):
    raise RuntimeError("Refusing a disk that is not exactly 40 GiB")
if subprocess.check_output(["findmnt", "-n", "-o", "FSTYPE", "/"], text=True).strip() != "tmpfs":
    raise RuntimeError("Expected the live installer's temporary root filesystem")
if subprocess.run(["blkid", "/dev/vda"], stdout=subprocess.DEVNULL).returncode == 0:
    raise RuntimeError("Refusing a disk that already contains a filesystem or partition table")
efi = Path("/sys/firmware/efi").exists()
run("parted", "-s", "/dev/vda", "mklabel", "gpt")
if efi:
    run("parted", "-s", "/dev/vda", "mkpart", "ESP", "fat32", "1MiB", "1025MiB")
    run("parted", "-s", "/dev/vda", "set", "1", "esp", "on")
else:
    run("parted", "-s", "/dev/vda", "mkpart", "BIOS", "1MiB", "3MiB")
    run("parted", "-s", "/dev/vda", "set", "1", "bios_grub", "on")
run("parted", "-s", "/dev/vda", "mkpart", "root", "ext4", "1025MiB", "100%")
run("udevadm", "settle")
run("mkfs.ext4", "-L", "RESPIN_TEST", "/dev/vda2")
run("mkdir", "-p", "/mnt")
run("mount", "/dev/vda2", "/mnt")
if efi:
    run("mkfs.fat", "-F", "32", "/dev/vda1")
    run("mkdir", "-p", "/mnt/boot")
    run("mount", "/dev/vda1", "/mnt/boot")

data = {
    "rootMountPoint": "/mnt", "firmwareType": "efi" if efi else "bios",
    "bootLoader": {"installPath": "/dev/vda"},
    "partitions": [{"mountPoint": "/", "fs": "ext4", "fsName": "ext4", "claimed": True, "device": "/dev/vda2"}],
    "hostname": "respin-test", "username": "respintest", "fullname": "Respin Test",
    "locationRegion": "Etc", "locationZone": "UTC",
    "packagechooser_packagechooser": "plasma6",
    "keyboardLayout": "us", "keyboardVariant": "", "keyboardVConsoleKeymap": "us",
    "nixos_allow_unfree": False,
}
if efi:
    data["partitions"].append({"mountPoint": "/boot", "fs": "fat32", "fsName": "fat32", "claimed": True, "device": "/dev/vda1"})


def host_process(argv, cwd=None, input_data=None):
    if argv == ["cp", "/dev/stdin", "/mnt/etc/nixos/configuration.nix"]:
        head, end = input_data.rsplit("}", 1)
        input_data = head + '''
  # QEMU test diagnostics only; never emitted by the normal GUI installer.
  services.qemuGuest.enable = true;
  boot.kernelParams = [ "console=ttyS0,115200n8" "console=tty0" ];
}''' + end
    result = subprocess.run(argv, cwd=cwd, input=input_data, text=True, capture_output=True, check=True)
    return result.stdout


last_progress = -1


def report_progress(value):
    global last_progress
    step = int(value * 1000)
    if step > last_progress:
        print(f"PROGRESS {value:.3f}", flush=True)
        last_progress = step


fake = types.ModuleType("libcalamares")
fake.globalstorage = types.SimpleNamespace(value=data.get)
fake.job = types.SimpleNamespace(setprogress=report_progress)
fake.utils = types.SimpleNamespace(
    gettext_path=lambda: "/usr/share/locale", gettext_languages=lambda: ["en"],
    debug=lambda message: print(message, flush=True),
    warning=lambda message: print("WARNING", message, flush=True),
    error=lambda message: print("ERROR", message, flush=True),
    host_env_process_output=host_process,
)
sys.modules["libcalamares"] = fake
module_path = Path("/run/current-system/sw/lib/calamares/modules/nixos/main.py")
if not module_path.is_file():
    module_path = next(path for path in Path("/nix/store").glob("*-calamares-nixos-extensions-*/lib/calamares/modules/nixos/main.py")
                       if "target_flake" in path.read_text())
assert module_path.is_file(), module_path
assert "target_flake" in module_path.read_text()
spec = importlib.util.spec_from_file_location("nixos_job", module_path)
module = importlib.util.module_from_spec(spec)
spec.loader.exec_module(module)
failure = module.run()
if failure is not None:
    raise RuntimeError(failure)
assert Path("/mnt/etc/nixos/flake.lock").read_bytes() == Path("/etc/determinate-installer/flake.lock").read_bytes()
assert 'nixosConfigurations."respin-test"' in Path("/mnt/etc/nixos/flake.nix").read_text()
run("sync")
print("CALAMARES_INSTALL_PASS", flush=True)
