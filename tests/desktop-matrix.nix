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
    modules = [
      flake.inputs.determinate.nixosModules.default
      module
      {
        fileSystems."/" = {
          device = "/dev/disk/by-label/TEST_ROOT";
          fsType = "ext4";
        };
        fileSystems."/boot" = {
          device = "/dev/disk/by-label/TEST_BOOT";
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
{
  derivation = system.config.system.build.toplevel.drvPath;
  defaultSession = system.config.services.displayManager.defaultSession;
  sessions = system.config.services.displayManager.sessionData.sessionNames;
  zone = system.config.time.timeZone;
  supplicantEnabled = system.config.networking.wireless.enable;
  redistributableFirmware = system.config.hardware.enableRedistributableFirmware;
  allowUnfree = system.config.nixpkgs.config.allowUnfree;
}
