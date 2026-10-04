# Adapted from DeterminateSystems/nixos-iso (Apache-2.0).
# Official graphical base, native Rust installer, unchanged system input lock.
{
  description = "Unofficial NixOS graphical installer with Determinate Nix and Rust Calamares";
  inputs.nixpkgs.url = "https://flakehub.com/f/NixOS/nixpkgs/0.1";
  inputs.determinate.url = "https://flakehub.com/f/DeterminateSystems/determinate/*";
  inputs.fh.url = "https://flakehub.com/f/DeterminateSystems/fh/*.tar.gz";

  outputs =
    inputs@{
      self,
      nixpkgs,
      determinate,
      fh,
      ...
    }:
    let
      system = "x86_64-linux";
      pkgs = nixpkgs.legacyPackages.${system};
      tools = import ./nix/tools.nix { inherit pkgs; };
      calamares = import ./nix/calamares.nix { inherit pkgs; };
      installer = nixpkgs.lib.nixosSystem {
        inherit system;
        specialArgs = { inherit inputs; };
        modules = [
          determinate.nixosModules.default
          "${nixpkgs}/nixos/modules/installer/cd-dvd/installation-cd-graphical-base.nix"
          ./modules/installer.nix
        ];
      };
      etc = installer.config.environment.etc;
      liveConfigs = [
        installer.config
      ]
      ++ builtins.map (s: s.configuration) (builtins.attrValues installer.config.specialisation);
      menuLabel =
        c:
        c.isoImage.prependToMenuLabel
        + c.system.nixos.distroName
        + " "
        + c.system.nixos.label
        + c.isoImage.appendToMenuLabel;
    in
    {
      nixosConfigurations.installer = installer;
      packages.${system} = {
        default = self.packages.${system}.iso;
        iso = installer.config.system.build.isoImage;
        inherit tools calamares;
      };
      checks.${system} = {
        native-installer = calamares;
        live-profiles =
          let
            normal = installer.config;
            lts = normal.specialisation.lts_kernel.configuration;
            recovery = normal.specialisation.compatibility.configuration;
          in
          assert normal.boot.kernelPackages.kernel.version == pkgs.linuxPackages_latest.kernel.version;
          assert lts.boot.kernelPackages.kernel.version == pkgs.linuxPackages.kernel.version;
          assert recovery.boot.kernelPackages.kernel.version == normal.boot.kernelPackages.kernel.version;
          assert normal.services.displayManager.defaultSession == "plasma";
          assert recovery.services.displayManager.defaultSession == "xfce";
          assert recovery.services.xserver.displayManager.lightdm.enable;
          assert !recovery.services.desktopManager.plasma6.enable;
          assert !recovery.services.displayManager.plasma-login-manager.enable;
          assert recovery.environment.sessionVariables.LIBGL_ALWAYS_SOFTWARE == "1";
          assert
            recovery.environment.etc."calamares-nixos/settings.json".text
            == etc."calamares-nixos/settings.json".text;
          assert builtins.all (
            c:
            builtins.stringLength (menuLabel c) <= 64
            && pkgs.lib.hasPrefix c.boot.kernelPackages.kernel.version (menuLabel c)
            && c.isoImage.configurationName == null
            && c.services.libinput.enable
            && !c.boot.plymouth.enable
            && !(c.environment.etc ? "nixos-generate-config.conf")
          ) liveConfigs;
          pkgs.writeText "live-profile-checks.json" (
            builtins.toJSON (
              builtins.map (c: {
                label = menuLabel c;
                kernel = c.boot.kernelPackages.kernel.version;
                session = c.services.displayManager.defaultSession;
              }) liveConfigs
            )
          );
        template-lock = pkgs.runCommand "template-lock-tests" { } ''
          ${tools}/bin/respin-tools check-config template ${./templates/flake.nix.in} ${./flake.lock}
          touch $out
        '';
        kernel-settings =
          let
            lts =
              pkgs.writeText "lts-settings.json"
                installer.config.specialisation.lts_kernel.configuration.environment.etc."calamares-nixos/settings.json".text;
            latest = pkgs.writeText "latest-settings.json" etc."calamares-nixos/settings.json".text;
          in
          assert !(etc ? "nixos-generate-config.conf");
          assert !(etc ? "determinate-installer/flake.lock");
          assert builtins.all (
            p:
            !(builtins.elem (pkgs.lib.getName p) [
              "calamares-nixos"
              "calamares-nixos-extensions"
              "respin-tools"
            ])
          ) installer.config.environment.systemPackages;
          pkgs.runCommand "native-kernel-settings-tests" { } ''
            ${tools}/bin/respin-tools check-config settings ${lts} ${latest}
            touch $out
          '';
      };
      formatter.${system} = pkgs.nixfmt;
    };
}
