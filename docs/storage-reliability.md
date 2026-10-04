# Filesystem selection and NVMe mount reliability

On 2026-10-04 the user reported `mount` exit status 32 on
`/dev/nvme0n1p2` during installation on a ThinkPad X1 Carbon Gen 14.
The screenshot shows failure at the root mount stage, after the ext4 formatter
returned success. It does not include the kernel message or the disk's previous
filesystem, so the exact physical cause has not been established.

## Gaps in the preceding implementation and tests

The old installer wiped the whole disk's signatures, created partitions,
formatted ext4, then immediately used an untyped `mount`. Wiping a partition
table does not wipe filesystem signatures within partitions. Udev could probe
while formatting, and its cached type could describe the previous filesystem.
A successful formatter exit was not followed by an uncached type check.

The previous QEMU runner always used a blank 40 GiB VirtIO disk. It did not
exercise NVMe, 4Kn namespaces, old filesystem signatures or reinstallation.
Its historical successful installs do not contradict the physical failure.

These changes address those concrete gaps; they are not a claim that the
screenshot proves a specific race or that hardware failures are impossible.
See [wipefs](https://man7.org/linux/man-pages/man8/wipefs.8.html),
[blkid probing](https://man7.org/linux/man-pages/man8/blkid.8.html) and
[systemd block-device locking](https://systemd.io/BLOCK_DEVICE_LOCKING/).

## Changes

- The GUI and immutable reviewed plan carry ext4 (default), Btrfs or XFS.
  Btrfs uses a single root volume with Zstd compression retained after reboot.
  Automatic snapshots and subvolume layouts are not configured. UEFI uses FAT32.
- Before erasing, the helper checks the selected formatter and live-kernel
  filesystem support. All live profiles explicitly include ext4/Btrfs/XFS/VFAT.
- The whole-device lock spans partitioning and formatting. The helper rereads
  the table and checks actual kernel partition parents, numbers and geometry.
  It wipes signatures on each new partition, formats, and verifies the result
  with uncached `blkid --probe`, refusing ambiguity or a wrong type.
- After releasing the device lock, it triggers fresh udev probing and waits for
  UUID metadata, rechecks availability, then mounts with an explicit type.
  No automatic reformat/retry hides an I/O or mount failure.
- Failures save a root-only `/run/calamares-storage-*.log` containing bounded
  partition/probe/kernel diagnostics. The GUI displays selectable, scrollable
  failure details. Logs must be copied before rebooting.
- The ISO source pin is updated to the companion fix; the installed system's
  existing three-input lock remains unchanged.

Pinned installer: `dc4127509d37c0e0d8144159744af0282a165127`,
source hash `sha256-lg7J1f25Il7kr94RvTztHaRUx+dgNx89PvUw6/1CgOc=`.
The source archive was fetched and hashed with Nix, then successfully imported
through the ISO's normal `fetchFromGitHub` path.

## Regression coverage

The package exposes a separate NixOS storage VM check, included in both
repositories' `nix flake check`. It runs real formatting, uncached probing,
mounting, writes, unmounting and remounting on newly created regular-file-backed
loop devices. It covers all nine old/new ext4/Btrfs/XFS pairs with 512-byte and
4096-byte sectors and FAT32 mounts at both sizes. Normal unprivileged Cargo
runs explicitly report this privileged test as ignored. The Nix check invokes
it explicitly; it can use emulation when nested KVM is unavailable.

Backend full-ISO tests now default to NVMe with an existing Btrfs filesystem,
installing ext4. Options are `--disk-bus virtio|nvme|nvme4k`,
`--filesystem ext4|btrfs|xfs`, and
`--previous-filesystem blank|ext4|btrfs|xfs`. The fixture first checks invalid
requests and preflight on the blank disposable disk, then seeds the old
filesystem and populates probe metadata before invoking the packaged helper.
The disk serial and size guards apply to both transports. Reboot verification
checks the requested filesystem, persistent device paths and Btrfs compression.
The runner preserves the controller and filesystem when resuming a boot test.

Run the complete release matrix against a rebuilt ISO on x86_64 Linux with KVM:

```sh
rust-script --force scripts/qemu_storage_matrix.rs /path/to/candidate.iso
```

It performs ten complete installs and disk-only boots: all three filesystems on
UEFI NVMe, UEFI 4Kn NVMe and BIOS VirtIO, plus blank VirtIO/ext4. It stops on the
first failure and preserves the normal per-run JSON, logs and screenshots.
GUI tests use a blank disk and verify the filesystem selected in the runner.

## Validation on 2026-10-04

Completed in a disposable Linux container on the macOS development host
(OrbStack kernel `7.0.14-orbstack-00380-ga7e0a2dc9535`, x86_64 Rust userspace):

- All 18 real filesystem replacement/remount cases passed, plus both FAT32
  mounts. Only temporary image-backed loop devices were formatted.
  [Storage matrix output](test-results/filesystem-storage-matrix.log).
- Full GTK/network build: 33 ordinary runtime tests and six compile-fail
  doctests passed. The privileged matrix was run separately and passed.
  [Native test output](test-results/filesystem-native-tests.log).
- Clippy with warnings denied passed for all Linux targets; the separate VM
  fixture compiles. The orchestration suite passed all 15 tests.
- Rust formatting and patch-whitespace checks passed. The release-matrix
  script compiled and rejected a missing ISO argument before creating a VM.
- QEMU 7.2.22 accepted the 4096-byte NVMe device configuration in a paused
  launch. This is a device-configuration check, not an installed-system boot.
- The NixOS VM storage check built and passed the real matrix under QEMU
  emulation, then passed again through the ISO repository's exact published
  source pin (179 seconds, kernel 6.18.54).
  [NixOS VM result](test-results/filesystem-nixos-vm.json).
  The outer disposable container
  required disabling Nix's incompatible seccomp filter and sandbox; source
  hashes and normal cache verification remained enabled.
- The pinned NixOS configurations evaluate with all four filesystem drivers
  enabled in all three live profiles.

UBS was also run and reviewed. It is not a clean scanner gate: its five existing
critical matches flag the fixed-command wrapper, synthetic unit-test password,
UID/libnm return-code comparisons and GUI password confirmation. The added
warnings are principally assertions/unwraps in tests and bounded allocations;
they were reviewed with the command ordering and cleanup paths. No broad
suppression rules were added.

Outstanding release evidence: a rebuilt final ISO, the ten complete-ISO
install/reboot cases and the user's physical NVMe retest. The successful loop
matrix does not establish controller firmware behavior or final-image bootability.
The previous ISO and USB verification reports remain historical evidence.
