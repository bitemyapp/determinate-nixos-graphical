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
assert system.config.fileSystems."/".device == "/dev/disk/by-uuid/11111111-2222-3333-4444-555555555555";
assert system.config.fileSystems."/boot".device == "/dev/disk/by-uuid/A1B2-C3D4";
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
