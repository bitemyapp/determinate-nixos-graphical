# Live media only; never imported by the installed machine.
# Plasma setup follows the official NixOS graphical installer module (MIT).
{
  config,
  lib,
  pkgs,
  inputs,
  ...
}:
let
  calamares = import ../nix/calamares.nix { inherit pkgs; };
  settings =
    kernel:
    builtins.toJSON {
      template_dir = "/etc/calamares-nixos";
      zoneinfo = "${pkgs.tzdata}/share/zoneinfo";
      state_version = config.system.nixos.release;
      inherit kernel;
      test_diagnostics = false;
    };
in
{
  services.desktopManager.plasma6 = {
    enable = true;
    enableQt5Integration = false;
  };
  services.displayManager = {
    plasma-login-manager.enable = true;
    autoLogin = {
      enable = true;
      user = "nixos";
    };
  };
  environment.plasma6.excludePackages = [ pkgs.kdePackages.plasma-workspace-wallpapers ];
  programs.kde-pim.enable = false;
  environment.systemPackages = [
    inputs.fh.packages.${pkgs.stdenv.hostPlatform.system}.default
    calamares
    (pkgs.makeAutostartItem {
      name = "org.calamares.NixOSRust";
      package = calamares;
    })
  ];
  # No blanket wheel grant from the graphical base. Only the local active live
  # user may start this exact helper without a password; other policies remain.
  security.polkit.enable = true;
  security.polkit.enablePkexecWrapper = true;
  security.polkit.extraConfig = lib.mkForce ''
    polkit.addRule(function(action, subject) {
      if (action.id == "org.calamares.nixos.install" &&
          subject.user == "nixos" && subject.local && subject.active) {
        return polkit.Result.YES;
      }
    });
  '';
  networking.wireless.enable = lib.mkForce false;
  networking.networkmanager.enable = true;

  # The native backend writes the target flake and explicitly selects it.
  # No Python patch, global-storage bridge or nixos-generate-config override.
  environment.etc."calamares-nixos/flake.nix.in".source = ../templates/flake.nix.in;
  environment.etc."calamares-nixos/flake.lock".source = ../flake.lock;
  environment.etc."calamares-nixos/settings.json".text = settings "lts";
  systemd.tmpfiles.settings."10-installer-desktop" = {
    "/home/nixos/Desktop".d = {
      user = "nixos";
      group = "users";
      mode = "0755";
    };
    "/home/nixos/Desktop/nixos-manual.desktop"."L+".argument =
      "/run/current-system/sw/share/applications/nixos-manual.desktop";
    "/home/nixos/Desktop/gparted.desktop"."L+".argument =
      "${pkgs.gparted}/share/applications/gparted.desktop";
    "/home/nixos/Desktop/org.calamares.NixOSRust.desktop"."L+".argument =
      "${calamares}/share/applications/org.calamares.NixOSRust.desktop";
  };

  isoImage.edition = lib.mkForce "plasma-determinate-rust";
  isoImage.volumeID = "nixos-determinate-x86_64";
  image.baseName = lib.mkForce "nixos-graphical-determinate-rust-${config.system.nixos.label}-x86_64-linux";
  isoImage.appendToMenuLabel = " Rust Graphical Installer with Determinate";
  isoImage.configurationName = "Plasma (Linux LTS)";
  specialisation.latest_kernel.configuration = { config, lib, ... }: {
    imports = [ "${inputs.nixpkgs}/nixos/modules/installer/cd-dvd/latest-kernel.nix" ];
    isoImage.configurationName = lib.mkForce "Plasma (Linux ${config.boot.kernelPackages.kernel.version})";
    environment.etc."calamares-nixos/settings.json".text = lib.mkForce (settings "latest");
  };
  isoImage.squashfsCompression = "zstd -Xcompression-level 6";
  boot.kernelParams = [
    "console=ttyS0,115200n8"
    "console=tty0"
  ];
}
