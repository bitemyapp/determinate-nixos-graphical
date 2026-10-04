# Evaluate real configuration.nix output from `application-configurations`.
{
  caseName,
  configurations ? null,
}:
let
  repo = toString ../.;
  flake = builtins.getFlake ("git+file://" + repo);
  library = import ../nix/applications.nix {
    inputs = flake.inputs;
    calamares = flake.packages.x86_64-linux.calamares;
  };
  lib = flake.inputs.nixpkgs.lib;
  requested =
    if caseName == "all" then
      map (app: app.id) library.catalog
    else if caseName == "none" then
      [ ]
    else if caseName == "free-only" then
      map (app: app.id) (builtins.filter (app: !(app.unfree or false)) library.catalog)
    else
      [ caseName ] ++ ((lib.findFirst (app: app.id == caseName) null library.catalog).requires or [ ]);
  expected = map (app: app.id) (
    builtins.filter (app: builtins.elem app.id requested) library.catalog
  );
  system = flake.inputs.nixpkgs.lib.nixosSystem {
    system = "x86_64-linux";
    inherit (library) specialArgs;
    modules = [
      flake.inputs.determinate.nixosModules.default
      "${flake.packages.x86_64-linux.calamares.src}/src/applications.nix"
      (builtins.toPath (repo + "/.work/application-modules/" + caseName + ".nix"))
      {
        fileSystems."/" = {
          device = "/dev/disk/by-uuid/old-root";
          fsType = "ext4";
        };
        fileSystems."/boot" = {
          device = "/dev/disk/by-uuid/old-boot";
          fsType = "vfat";
        };
      }
    ];
  };
  c = system.config;
  has = id: builtins.elem id expected;
  # Inspect the manifest without realizing its referenced packages here.
  manifest = builtins.fromJSON (
    builtins.unsafeDiscardStringContext c.environment.etc."installer-applications.json".text
  );
in
assert c.calamares.applications == expected;
assert manifest.selected == expected;
assert c.programs.firefox.enable == has "firefox";
assert c.programs.steam.enable == has "steam";
assert c.virtualisation.docker.rootless.enable == has "docker";
assert !(builtins.elem "docker" c.users.users.alice.extraGroups);
assert !has "rustup" || has "build-tools";
assert c.fileSystems."/".device == "/dev/disk/by-uuid/11111111-2222-3333-4444-555555555555";
assert c.fileSystems."/boot".device == "/dev/disk/by-uuid/A1B2-C3D4";
{
  derivation = c.system.build.toplevel.drvPath;
  inherit manifest;
  allowUnfree = c.nixpkgs.config.allowUnfree;
  rootlessDocker = c.virtualisation.docker.rootless.enable;
  dockerLinger = c.users.users.alice.linger;
}
