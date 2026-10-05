# Fast installation, tuning and Hyprland handoff

Updated October 5, 2026, America/Chicago. The previous SanDisk/filesystem
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

## October 5 round: hardware feedback fixes (uncommitted)

The user installed the final image on a ThinkPad X1 Carbon Gen 14 with every
desktop except Cinnamon plus many applications. Their reports and the fixes:

| Report | Cause | Fix |
|---|---|---|
| Ctrl+Alt+F*n* does nothing | ananicy-cpp's `cgroup_realtime_workaround` (forced on by nixpkgs) moved realtime compositors to the root cgroup; polkit then denied logind `Seat.SwitchTo` | calamares `rust/system/tuning.nix`: ananicy cgroup features off |
| Plasma/GNOME failing or slow to start after switching desktops | SDDM's Wayland greeter (enabled by the Plasma module) races kwallet-pam when starting a session ([sddm#1443](https://github.com/sddm/sddm/issues/1443)): 30 s stall, then an inactive session on a black VT. Reproduced once in nine VM logins | `rust/system/desktops.nix`: `sddm.wayland.enable = false` (X11 greeter); eight further logins across Plasma, GNOME, Omarchy, Hyprland and MATE were all active at once |
| Omarchy "not quite as nice as the official one" | — | Omarchy session conformed to Quattro (v4.0.4) with the Waybar/Walker/Mako stack and the Rust helper; see `docs/hyprland.md` |
| Timezone should be a map with a sensible default | — | calamares `src/zonemap.rs`, `src/ui/zonemap.rs`, `src/timezone.rs`: clickable Natural Earth map, CLDR names, detection live → geoip.kde.org/ipinfo.io → hardware clock → New York. ipapi.co was rate-limiting (HTTP 429) |

Also fixed: Xfce's polkit-gnome agent and GNOME's IBus autostart no longer
start in other desktops, and Omarchy's dark GTK defaults are a dconf profile
selected only in its session (`DCONF_PROFILE` through uwsm's `env-omarchy`).

Verified on candidate `4c236fb0…` (impure build of both working trees, before
the SDDM change):

- All-inclusive install passed the harness: Plasma, GNOME, Xfce, MATE, LXQt,
  Hyprland and Omarchy with all applications, 108 s from click to completion
  for a 34.2 GB closure. That is VM disk-bound: 104 s copying.
- GUI-driven install through the new Location page: detection by
  geoip.kde.org, map clicks (London → BST, back to Chicago), and the installed
  `time.timeZone = "America/Chicago"`. The harness check failed only because
  my click on Btrfs missed, so it installed the default ext4 against
  `--filesystem btrfs`.
- VT switching to tty3 and back works in Plasma, GNOME and Omarchy.
- The SDDM change was applied with `nixos-rebuild boot` in the installed
  all-inclusive VM and exercised with the login rotation above.

Screenshots: `.work/candidate-screens/` and `.work/timezone-screens/`.
These changes are committed and pushed: calamares `1e14e4d`, pinned here. The
final image built from them is described under "Candidate images".

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
`nix/calamares-source.json`: Calamares `1e14e4d448656b3b5f117dabd1c78b4ade2ca1bc`
on `claude/fast-install-hyprland`, pushed to GitHub (the October 5 fixes on top
of `cd054b9`). Both branches are pushed; neither is merged.

## Candidate images

**Final image** (application cache plus all eight prebuilt desktops): a pure
build of this repository at its pinned installer `1e14e4d`, without overrides.
`nixos-graphical-determinate-rust-26.11.20261001.c59305b-x86_64-linux.iso`,
11,910,500,352 bytes, SHA-256
`4f7621f035767d4ab78609d4b3ad724b457605af847ad593214fee004cf99cb1`.
It is on the workstation at `~/work/dng/pure-iso/iso/` and on the Mac at
`artifacts/fast-install-oct5/`. `.work/flash-sandisk-fast-install.command`
writes it to the SanDisk.
- `nix flake check` passed, including the storage VM test.
- 191 desktop and 34 application configurations passed.
- An all-inclusive VM install passed: Plasma, GNOME, Xfce, MATE, LXQt,
  Hyprland and Omarchy with all 31 applications. It took 130.4 s from click to
  completion for a 34.2 GB closure, while the configuration matrices ran on
  the same host.
- Evidence: `docs/test-results/fast-install-*.json` and
  `docs/test-results/application-configurations.json`.

The previous final image, `69d989e3…` at installer `cd054b9`, is the one the
user installed on the X1 Carbon. It remains in `artifacts/fast-install/`.

`bce81f55…`, which carried the GUI screenshots and the other VM runs, differs
from this image only by a storage-test ordering fix (test code). That fix
changes the source hash, and therefore the image hash.

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
