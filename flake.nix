# Adapted from DeterminateSystems/nixos-iso (Apache-2.0).
# Changes: official Plasma/Calamares frontend, pinned target flake, and tests.
{
  description = "Unofficial NixOS graphical installer with Determinate Nix";

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
      installer = nixpkgs.lib.nixosSystem {
        inherit system;
        specialArgs = { inherit inputs; };
        modules = [
          determinate.nixosModules.default
          "${nixpkgs}/nixos/modules/installer/cd-dvd/installation-cd-graphical-calamares-plasma6.nix"
          ./modules/installer.nix
        ];
      };
    in
    {
      nixosConfigurations.installer = installer;
      packages.${system} = {
        default = self.packages.${system}.iso;
        iso = installer.config.system.build.isoImage;
        calamares-extensions = installer.pkgs.calamares-nixos-extensions;
      };
      checks.${system} = {
        generate-config =
          pkgs.runCommand "generate-config-tests" { nativeBuildInputs = [ pkgs.python3 ]; }
            ''
              mkdir -p "$TMPDIR/target"
              ${installer.config.system.build.nixos-generate-config}/bin/nixos-generate-config \
                --root "$TMPDIR/target" --no-filesystems --flake
              python - "$TMPDIR/target/etc/nixos/flake.nix" ${./templates/flake.nix.in} <<'PY'
              import pathlib, sys
              actual = pathlib.Path(sys.argv[1]).read_text()
              expected = pathlib.Path(sys.argv[2]).read_text().replace("@HOSTNAME@", '"nixos"')
              assert actual.rstrip() == expected.rstrip(), actual
              PY
              touch $out
            '';
        calamares-handoff =
          pkgs.runCommand "calamares-handoff-tests"
            {
              nativeBuildInputs = [ pkgs.python3 ];
            }
            ''
              export PYTHONDONTWRITEBYTECODE=1
              python ${./tests/test_handoff.py} \
                ${./calamares/prepare_target.py} \
                ${./templates/flake.nix.in} ${./flake.lock} \
                ${installer.pkgs.calamares-nixos-extensions}/lib/calamares/modules/nixos/main.py
              touch $out
            '';
        kernel-defaults =
          let
            lts =
              pkgs.writeText "lts-defaults.ini"
                installer.config.environment.etc."nixos-generate-config.conf".text;
            latest =
              pkgs.writeText "latest-defaults.ini"
                installer.config.specialisation.latest_kernel.configuration.environment.etc."nixos-generate-config.conf".text;
          in
          pkgs.runCommand "kernel-defaults-tests" { nativeBuildInputs = [ pkgs.python3 ]; } ''
            python - ${lts} ${latest} <<'PY'
            import configparser, sys
            for path, kernel in zip(sys.argv[1:], ["lts", "latest"]):
                config = configparser.ConfigParser()
                config.read(path)
                assert config["Defaults"]["Flake"] == "1"
                assert config["Defaults"]["Kernel"] == kernel
            PY
            touch $out
          '';
      };
      formatter.${system} = pkgs.nixfmt;
    };
}
