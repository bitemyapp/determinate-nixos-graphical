# Fast installation, tuning and Hyprland handoff

Updated October 4, 2026, America/Chicago. The previous SanDisk/filesystem
handoff is archived at [archive/agent-handoff-sandisk-2026-10-04.md](archive/agent-handoff-sandisk-2026-10-04.md).

## User requirements for this round

1. Make the installer more attractive.
2. Port portable CachyOS performance and reliability ideas (no recompiled
   packages or custom kernel). Instead of zram, create a **swap partition
   matched to RAM with zswap**.
3. Make installation faster: under one minute from clicking Install, after all
   inputs are supplied, on real hardware. Precache in RAM without exceeding it.
4. Add **vanilla Hyprland** and an **Omarchy-flavored Hyprland**.
5. **All new code in Rust.** Approved exceptions: declarative Nix (modules,
   flake wiring, tests); declarative config and CSS files; Hyprland
   `hyprland.lua` limited to declarative `hl.*` calls.
6. Build, test and run VMs on `ssh wx-workstation`.
7. Capture screenshots of potential final candidates for the user.

## Where things are

Both repositories use the branch `claude/fast-install-hyprland`, created from
`codex/filesystem-install-reliability` and pushed to GitHub.

| Area | Location |
|---|---|
| Two-phase install (prepare → confirm → execute), timing record | calamares `rust/src/install.rs`, `helper.rs`, `session.rs` |
| RAM-sized swap, exact layout, chosen identities | calamares `rust/src/memory.rs`, `disk.rs` (`Layout`), `filesystem.rs` (`Identities`) |
| Page-cache warming | calamares `rust/src/precache.rs` |
| Nix progress parsing, dry-run sizes | calamares `rust/src/nixlog.rs` |
| Static installed-system modules (desktops, tuning, Hyprland, Omarchy and its Rust tool) | calamares `rust/system/`, embedded by `rust/build.rs` |
| Reference system for prebuilt media closures | calamares `rust/reference.nix` (sync test in `config.rs`) |
| libadwaita GUI | calamares `rust/src/ui.rs`, `rust/src/ui/`, `rust/data/style.css` |
| Media integration (reference systems, closure lists, zram, options) | graphical `modules/installer.nix`, `nix/reference-systems.nix`, `nix/calamares.nix` |
| VM harness changes (local fixture, sizing, USB throttle, timing) | graphical `rust/src/vm.rs`, `rust/src/builder.rs` |
| User-facing docs | graphical `README.md`, `docs/fast-install.md`, `docs/tuning.md`, `docs/hyprland.md` |

`nix/calamares.nix` accepts `RESPIN_CALAMARES_SRC=/abs/checkout` with
`--impure` for development builds. Pure builds use the pinned revision in
`nix/calamares-source.json`: Calamares `e825ac296833fb5faa942b5fd5f7b5afecb1da91`
on `claude/fast-install-hyprland`, pushed to GitHub. Both branches are pushed;
neither is merged.

## Candidate images

**Final candidate** (application cache plus all eight prebuilt desktops), built
impurely from the local installer tree:
`wx-workstation:~/work/dng/release-iso/iso/nixos-graphical-determinate-rust-26.11.20261001.c59305b-x86_64-linux.iso`,
11,913,035,776 bytes, SHA-256
`bce81f550fc460a07eccc6773414162d38d4af6fe736df29a871b2ab99cd7b90`.
A pure build from the pushed pin (`nix build .#iso`, no override) reproduces
this exact SHA-256. It differs from `1e1253a4…` (screenshots,
GUI-driven XFS install, BIOS/Omarchy install) only by readable Nix failure
reports.

The previous candidate `201a7c40…` (11,912,892,416 bytes) passed the first
GUI-driven install. The final candidate adds: zram hidden from the disk list,
the `/var/log/calamares-nixos/install.json` timing record, and `nix copy` in
place of the removed direct copier.

## Verified

- Calamares library: 57 unit tests, 4 GUI tests and 6 compile-fail doctests;
  Clippy with `-D warnings` and rustfmt are clean. Omarchy tool: 19 tests,
  clean Clippy. Harness: 16 tests.
- Configuration matrices: all 191 supported desktop combinations and all 34
  application selections evaluate against pinned NixOS. They assert the
  swap/zswap/resume/tuning wiring and that the default session exists.
- VM installs that booted the installed disk and passed the fixture's checks:
  Plasma (UEFI/NVMe/ext4), Xfce with swap and tuning off (BIOS/VirtIO/Btrfs),
  Omarchy + Hyprland (UEFI/VirtIO/Btrfs), and a **GUI-driven** install on the
  release candidate.
  - The GUI install used UEFI/NVMe/Btrfs with Plasma, vanilla Hyprland and
    Omarchy, Omarchy as the default.
  - It reported "Installed in 57 seconds" for an 11.4 GiB system, with 8 vCPUs
    and ISO reads throttled to 200 MB/s.
  - Login through SDDM into Omarchy worked automatically. Manual logins worked
    into vanilla Hyprland and Plasma.
  - Screenshots are in `docs/images/candidate-*.png`, and the full set is in
    `.work/final-screens/` (local, ignored).
- The final candidate passed a second GUI-driven install: UEFI/NVMe/XFS, Plasma
  plus vanilla Hyprland (default), and Install clicked before preparation
  finished. The installer waited, then installed. The timing record showed
  storage 1.1 s, copy 40.6 s for 9.8 GiB and bootloader 0.9 s. The harness
  logged into vanilla Hyprland. Screenshots are in
  `.work/final-screens/candidate-1e1253a4/`.
- Copier A/B on a quiet host with otherwise identical images: `nix copy` 30.5 s
  click-to-complete against 48.0 s for the direct copier, which was removed.
- Hyprland/Omarchy component behavior in standalone VMs: see `docs/hyprland.md`.

## Not verified / open

- **Physical hardware**, including the ThinkPad X1 Carbon Gen 14 and the
  one-minute target on real NVMe and USB. VM disk emulation limits copy
  throughput to about 250–450 MB/s.
- Hibernation and resume, brightness, Bluetooth, battery and hyprlauncher on
  real hardware.
- The deferred path is VM-verified with binary-cached applications on a 6 GiB
  VM. With applications that must be compiled from source (the AI apps
  without the media cache), a source build failed in the VM; release media
  carry those packages.
- Measuring the hardware result: after installation, boot the installed system
  and read `/var/log/calamares-nixos/install.json` for stage timings. This is
  present only in images built after that change.

## Workstation notes

- Nix is at `/nix/var/nix/profiles/default/bin`. The user is not a trusted Nix
  user and has no direct `/dev/kvm` access. Run QEMU in Docker with
  `--device /dev/kvm --group-add 993 -v /nix:/nix:ro`; the env file is
  `~/work/dng/vm.env`.
- Working copies: `~/work/dng/{calamares,determinate-nixos-graphical}`, rsynced
  from the Mac. `~/work/dng/dev-iso/flake.nix` is a scratch flake overriding
  `respin.referenceDesktops` and `respin.cacheApplications`.
- `/home/callen` is itself a Git repository. Use `path:` for scratch flakes.
- Never use `path:` for the graphical repository once VM artifacts exist: it
  copies the ignored `.work/` (tens of GB of VM disks) into the store. Use
  `git+file://` and `git add -A` in the workstation copy so new files are
  included.
- Delete a scratch flake's `flake.lock` before rebuilding. A stale lock keeps
  an old snapshot of a `path:` input.
- Nix inside Alpine containers needs a `HOME` owned by the container user.
- Matching `pgrep -f`/`pkill -f` patterns also match the invoking shell's own
  command line; prefer PIDs.
