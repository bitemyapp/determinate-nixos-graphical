# Evaluate actual configuration.nix strings emitted by the pinned Rust backend.
# Usage inside the rootless builder: nix eval --impure --json --file
# tests/desktop-matrix.nix --apply
# 'f: f { configurations = "/workspace/.work/desktop-configurations.json"; caseName = "plasma-xfce"; }'
{ configurations, caseName }:
let
  flake = builtins.getFlake "git+file:///workspace";
  # The driver writes and fsyncs these generated modules. Avoid adding ephemeral
  # test source files to the persistent Nix store just to evaluate them.
  module = builtins.toPath ("/workspace/.work/desktop-modules/" + caseName + ".nix");
  system = flake.inputs.nixpkgs.lib.nixosSystem {
    system = "x86_64-linux";
    specialArgs = {
      applicationPkgs = import flake.inputs.applications {
        system = "x86_64-linux";
        config.allowUnfree = true;
      };
      aiPackages = flake.inputs.ai-apps.packages.x86_64-linux;
      ompPackage = flake.inputs.omp.packages.x86_64-linux.omp;
    };
    modules = [
      flake.inputs.determinate.nixosModules.default
      # The modules installed systems import from their `calamares` input.
      flake.inputs.calamares.nixosModules.default
      module
      {
        fileSystems."/" = {
          # Simulate hardware detection choosing aliases from before a reformat.
          device = "/dev/disk/by-uuid/00000000-0000-0000-0000-000000000000";
          fsType = "ext4";
        };
        fileSystems."/boot" = {
          device = "/dev/disk/by-uuid/0000-0000";
          fsType = "vfat";
        };
      }
    ];
  };
in
assert system.config.networking.networkmanager.enable;
assert system.config.networking.wireless.enable;
assert system.config.networking.wireless.dbusControlled;
assert system.config.hardware.enableRedistributableFirmware;
assert system.config.nixpkgs.config.allowUnfree;
assert
  system.config.fileSystems."/".device == "/dev/disk/by-uuid/11111111-2222-4333-8444-555555555555";
assert system.config.fileSystems."/boot".device == "/dev/disk/by-uuid/A1B2-C3D4";
# RAM-sized swap partition with zswap and hibernation, and CachyOS-inspired tuning.
assert
  map (swap: swap.device) system.config.swapDevices == [
    "/dev/disk/by-uuid/66666666-7777-4888-9999-aaaaaaaaaaaa"
  ];
assert system.config.boot.resumeDevice == "/dev/disk/by-uuid/66666666-7777-4888-9999-aaaaaaaaaaaa";
assert builtins.elem "zswap.enabled=1" system.config.boot.kernelParams;
assert system.config.boot.kernel.sysctl."vm.swappiness" == 100;
assert system.config.boot.kernel.sysctl."vm.dirty_bytes" == 268435456;
assert system.config.services.ananicy.enable;
# Processes must stay in their systemd units (VT switching, logout cleanup).
assert !system.config.services.ananicy.settings.apply_cgroup;
assert !system.config.services.ananicy.settings.cgroup_load;
assert !system.config.services.ananicy.settings.cgroup_realtime_workaround;
# Xfce's polkit-gnome agent must not start in the other desktops.
assert
  !builtins.elem "xfce" system.config.calamares.desktops
  ||
    flake.inputs.nixpkgs.lib.hasInfix "OnlyShowIn=XFCE;"
      system.config.environment.etc."xdg/autostart/polkit-gnome-authentication-agent-1.desktop".text;
# SDDM's Wayland greeter races kwallet-pam when starting sessions (sddm#1443).
assert !system.config.services.displayManager.sddm.wayland.enable;
# GNOME's IBus autostart must skip the Hyprland sessions.
assert
  !(
    builtins.elem "gnome" system.config.calamares.desktops
    && (
      builtins.elem "hyprland" system.config.calamares.desktops
      || builtins.elem "omarchy" system.config.calamares.desktops
    )
  )
  ||
    flake.inputs.nixpkgs.lib.hasInfix "NotShowIn=GNOME;KDE;Hyprland;"
      system.config.environment.etc."xdg/autostart/ibus-daemon.desktop".text;
assert builtins.elem system.config.services.displayManager.defaultSession
  system.config.services.displayManager.sessionData.sessionNames;
{
  derivation = system.config.system.build.toplevel.drvPath;
  defaultSession = system.config.services.displayManager.defaultSession;
  sessions = system.config.services.displayManager.sessionData.sessionNames;
  zone = system.config.time.timeZone;
  supplicantEnabled = system.config.networking.wireless.enable;
  redistributableFirmware = system.config.hardware.enableRedistributableFirmware;
  allowUnfree = system.config.nixpkgs.config.allowUnfree;
  rootDevice = system.config.fileSystems."/".device;
  bootDevice = system.config.fileSystems."/boot".device;
}
