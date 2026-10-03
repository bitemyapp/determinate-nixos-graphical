#!/usr/bin/env bash
# Runs INSIDE the disposable builder VM, never on the host.
set -euo pipefail
test "$(cat /sys/class/block/vda/serial)" = RESPIN_BUILDER_ONLY
test "$(blockdev --getsize64 /dev/vda)" = 107374182400
test "$(findmnt -n -o FSTYPE /)" = tmpfs
if ! blkid /dev/vda; then
  mkfs.ext4 -L respin-build /dev/vda
fi
test "$(blkid -s LABEL -o value /dev/vda)" = respin-build
mkdir -p /build
mount /dev/vda /build
# Move the writable Nix store out of the live ISO's RAM overlay.
systemctl stop determinate-nixd.socket nix-daemon.socket nix-daemon.service
if [ ! -f /build/store-ready ]; then
  mkdir -p /build/nix
  cp -a /nix/store /nix/var /build/nix/
  touch /build/store-ready
fi
mount --bind /build/nix /nix
systemctl start determinate-nixd.socket nix-daemon.socket
mkdir -p /build/tmp /root/.ssh
cp /workspace/.work/rootless/ssh-key.pub /root/.ssh/authorized_keys
chmod 700 /root/.ssh
chmod 600 /root/.ssh/authorized_keys
git config --global --add safe.directory /workspace
echo RESPIN_BUILDER_READY
