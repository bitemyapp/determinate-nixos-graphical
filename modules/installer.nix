# Determinate integration follows DeterminateSystems/nixos-iso (Apache-2.0).
# This module only applies to the live installer, never the installed system.
{
  config,
  lib,
  pkgs,
  inputs,
  ...
}:
{
  nixpkgs.overlays = [
    (final: prev: {
      calamares-nixos-extensions = prev.calamares-nixos-extensions.overrideAttrs (old: {
        patches = (old.patches or [ ]) ++ [ ../patches/calamares-flake-install.patch ];
        postPatch = (old.postPatch or "") + ''
          substituteInPlace modules/nixos/main.py \
            --replace-fail '@prepareTarget@' '${../calamares/prepare_target.py}'
        '';
      });
    })
  ];

  environment.systemPackages = [ inputs.fh.packages.${pkgs.stdenv.hostPlatform.system}.default ];
  networking.wireless.enable = lib.mkForce false;
  networking.networkmanager.enable = true;

  # Carry exactly the same revisions into the installed machine. Calamares
  # replaces only @HOSTNAME@ and explicitly passes --flake to nixos-install.
  environment.etc."determinate-installer/flake.nix.in".source = ../templates/flake.nix.in;
  environment.etc."determinate-installer/flake.lock".source = ../flake.lock;
  environment.etc."nixos-generate-config.conf".text = ''
    [Defaults]
    Flake=1
    Kernel=lts
  '';
  # This option is inserted into an interpolating Perl heredoc, not written
  # directly as Nix. Protect literal Nix interpolation and Perl sigils.
  system.nixos-generate-config.flake = lib.replaceStrings [ "\\" "$" "@" ] [ "\\\\" "\\$" "\\@" ] (
    lib.replaceStrings [ "@HOSTNAME@" ] [ ''"nixos"'' ] (builtins.readFile ../templates/flake.nix.in)
  );

  isoImage.edition = lib.mkForce "plasma-determinate";
  isoImage.volumeID = "nixos-determinate-x86_64";
  image.baseName = lib.mkForce "nixos-graphical-determinate-${config.system.nixos.label}-x86_64-linux";
  isoImage.appendToMenuLabel = " Graphical Installer with Determinate";
  isoImage.configurationName = "Plasma (Linux LTS)";
  specialisation.latest_kernel.configuration =
    { config, lib, ... }:
    {
      imports = [ "${inputs.nixpkgs}/nixos/modules/installer/cd-dvd/latest-kernel.nix" ];
      isoImage.configurationName = lib.mkForce "Plasma (Linux ${config.boot.kernelPackages.kernel.version})";
      # Avoid merging two [Defaults] sections: Python's ConfigParser rejects
      # duplicate sections, which would otherwise break the Calamares job.
      environment.etc."nixos-generate-config.conf".text = lib.mkForce ''
        [Defaults]
        Flake=1
        Kernel=latest
      '';
    };
  # A smaller compression budget makes local rebuilds practical.
  isoImage.squashfsCompression = "zstd -Xcompression-level 6";

  # Permit serial diagnostics without removing the normal graphical console.
  boot.kernelParams = [
    "console=ttyS0,115200n8"
    "console=tty0"
  ];
}
