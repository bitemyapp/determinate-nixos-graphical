# Optional applications

The installer presents 31 choices with checkboxes, categories, search and short
descriptions. Firefox starts selected. Empty selection is supported. The review
screen lists the normalized selection, and the privileged helper validates it
again. Unknown IDs, duplicate requests and proprietary selections without
unfree-software consent are rejected before disk writes.

The catalog lives in the pinned installer's `applications.json`, shared by its
Rust UI/parser and generated NixOS module. Each installed system retains that
catalog and `applications.nix` beside its flake. `/etc/installer-applications.json`
records selected IDs and exact package versions and store paths.

## Sources

All sources are pinned in `flake.lock`. Adding the application inputs preserves
the existing base Nixpkgs, Determinate and FlakeHub CLI revisions. The application
pins were selected on 2026-10-04:

| Source | Pin | Use |
| --- | --- | --- |
| [NixOS/nixpkgs](https://github.com/NixOS/nixpkgs) | `a7868a727837f3c09cee2ce0ca671c76b1589fed` | Browsers, editors, terminals, developer tools, productivity, communication and media |
| [Numtide llm-agents.nix](https://github.com/numtide/llm-agents.nix) | `372f0337e8170e55ff0c017cd43ab73b02a062ad` | Codex, ChatGPT, Claude Code, Claude Desktop and OpenCode |
| [oh-my-pi upstream](https://github.com/can1357/oh-my-pi) | v18.6.1, `2a2c6dcbbb558c0f8145f67f28b3370984f2bf60` | oh-my-pi |

The AI definitions use upstream release artifacts or source and adapt them for
NixOS. The ChatGPT and Claude entries package their official Linux applications;
they are not unofficial web wrappers. Their vendor-supported Linux distributions
and this community NixOS packaging have different support scopes. Account login
is performed by the user after installation. The upstream oh-my-pi release was
newer than the Numtide package at the time of pinning.

OrbStack is macOS-only. At the user's request its Linux replacement is Docker
Engine + Compose. Thunderbird, KeePassXC and Gemini CLI are absent.

## Development tools

Development build tools includes GCC's C/C++ compilers and libc headers, Make,
binutils, CMake, Ninja, pkg-config, Git and patch. This covers the usual
`build-essential` role with common additional build drivers; projects can still
need their own libraries or a Nix development shell.

Selecting Rustup also selects and locks the build-tools checkbox. Deselecting
Rustup makes that checkbox optional again. Nixpkgs provides the NixOS-compatible
Rustup wrapper. Run `rustup default stable` after installation to download a
toolchain. Rustup is intentionally not accompanied by a conflicting system-wide
`rustc`/`cargo` package.

Docker runs as the installed user's rootless service, with a persistent user
manager and its socket selected in the user environment. The user is not added
to the privileged `docker` group. Compose is installed with the engine.

## Preparation and regression checks

The ISO includes a read-only store closure of all offered application packages.
They become installed applications only when selected. Before changing the
target disk, the helper resolves the selected closure against the immutable
lock, keeps it rooted, and checks target capacity against that closure plus a
24 GiB reserve for the base system and installation workspace. Package-resolution
failures stop before erasure.

`nix flake check` evaluates all choices and none, and builds the complete package
cache. The rootless builder's additional configuration matrix evaluates
actual Rust-generated configurations for every choice, all, none and free-only,
including dependency normalization, rootless Docker, Firefox/Steam integration
and freshly probed filesystem UUID overrides. It can also be run separately:

```sh
# In the /workspace Linux build environment, checks both desktop and app cases:
respin-tools check-selections
# Standalone application-only driver:
python3 scripts/check_application_matrix.py
```

The pinned Linux `calamares-vm-fixture` must first exist under
`.work/native-fixture/<revision>/bin`. Each configuration evaluates in its own
Nix process to bound memory usage.

The full ISO runner accepts `--applications all`, `none`, or comma-separated
catalog IDs. Additional application selections use an 80 GiB disposable disk;
Firefox-only and empty selections retain the 40 GiB disk. After booting without
the ISO, the fixture compares the installed manifest and verifies package paths.

`tests/applications-runtime.sh` is run as the disposable VM's regular test user
after an all-applications installation. It compiles and runs C/C++ programs with
Make and CMake/Ninja, downloads Rust stable using Rustup, builds Rust linked to a
C library, runs a rootless Compose workload, and checks the terminal assistants'
startup. It requires network access for the Rust toolchain and container image.
Desktop launch and visual checks are recorded separately from authenticated
service use, which requires the user's accounts.

See [filesystem reliability](storage-reliability.md) for the original NVMe
failure and the used-disk, 4Kn and legacy BIOS regression cases. Emulator passes
do not establish the behavior of the physical ThinkPad's controller or firmware.

## Validation in progress

The packaged installer passed 40 library tests, one GUI model test and six
compile-fail doctests. The separately invoked storage VM passed all 18 real
ext4/Btrfs/XFS replacement and remount cases (512-byte and 4096-byte sectors),
plus FAT32 checks. All 34 generated application configurations and all 47
desktop configurations evaluated successfully. The native orchestration tools
passed 15 tests and Clippy with warnings denied.

The complete application package builds, new ISO installation/runtime tests and
SanDisk write/read-back are still pending; the old filesystem-only image has
not been flashed as a substitute.
