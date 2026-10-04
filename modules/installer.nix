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
  diagnostics = import ../nix/live-diagnostics.nix { inherit pkgs; };
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
    diagnostics
    # Small interactive diagnostics, also used to prove real GUI event delivery
    # in our disposable VM tests (not merely successful QMP commands).
    pkgs.xev
    pkgs.xwininfo
    (pkgs.makeAutostartItem {
      name = "org.calamares.NixOSRust";
      package = calamares;
    })
  ];
  # No blanket wheel grant from the graphical base. Only the local active live
  # user may start this exact helper without a password. Keep live networking
  # usable too: mkForce replaces NetworkManager's normal group-based rule.
  security.polkit.enable = true;
  security.polkit.enablePkexecWrapper = true;
  security.polkit.extraConfig = lib.mkForce ''
    polkit.addRule(function(action, subject) {
      if (action.id == "org.calamares.nixos.install" &&
          subject.user == "nixos" && subject.local && subject.active) {
        return polkit.Result.YES;
      }
      if (action.id.indexOf("org.freedesktop.NetworkManager.") == 0 &&
          subject.user == "nixos" && subject.local && subject.active &&
          subject.isInGroup("networkmanager")) {
        return polkit.Result.YES;
      }
    });
  '';
  networking.wireless.enable = lib.mkForce false;
  networking.networkmanager.enable = true;
  services.libinput.enable = true;

  # Recent laptops need recent DRM, I2C and HID support. Keep the version pinned
  # by our existing lock; the older kernel is an explicitly named fallback.
  boot.kernelPackages = pkgs.linuxPackages_latest;
  boot.supportedFilesystems.zfs = lib.mkForce false;
  # Keep boot progress and the text-console recovery route visible.
  boot.plymouth.enable = lib.mkForce false;

  # The native backend writes the target flake and explicitly selects it.
  # No Python patch, global-storage bridge or nixos-generate-config override.
  environment.etc."calamares-nixos/flake.nix.in".source = ../templates/flake.nix.in;
  environment.etc."calamares-nixos/flake.lock".source = ../flake.lock;
  environment.etc."calamares-nixos/settings.json".text = settings "latest";
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
  # Put the distinguishing information FIRST. The upstream menu appends the
  # distro/revision; our old long branding hid kernel versions on small screens.
  isoImage.prependToMenuLabel = "${config.boot.kernelPackages.kernel.version} Plasma (default) | ";
  isoImage.appendToMenuLabel = lib.mkForce "";
  isoImage.configurationName = lib.mkForce null;
  specialisation.lts_kernel.configuration =
    {
      config,
      lib,
      pkgs,
      ...
    }:
    {
      boot.kernelPackages = lib.mkForce pkgs.linuxPackages;
      isoImage.prependToMenuLabel = lib.mkForce "${config.boot.kernelPackages.kernel.version} Plasma (LTS) | ";
      environment.etc."calamares-nixos/settings.json".text = lib.mkForce (settings "lts");
    };
  specialisation.compatibility.configuration = { config, lib, ... }: {
    isoImage.prependToMenuLabel = lib.mkForce "${config.boot.kernelPackages.kernel.version} Xfce/X11 (software) | ";
    services.desktopManager.plasma6.enable = lib.mkForce false;
    services.displayManager.plasma-login-manager.enable = lib.mkForce false;
    services.xserver.displayManager.lightdm.enable = true;
    services.xserver.desktopManager.xfce = {
      enable = true;
      enableScreensaver = false;
    };
    services.displayManager.defaultSession = lib.mkForce "xfce";
    environment.sessionVariables = {
      LIBGL_ALWAYS_SOFTWARE = "1";
      GSK_RENDERER = "cairo";
    };
    # Retain native kernel modesetting: nomodeset is a separate upstream boot
    # menu option, not a blanket workaround for all graphics hardware.
    services.xserver.deviceSection = ''
      Option "AccelMethod" "none"
      Option "SWcursor" "true"
    '';
    environment.etc."xdg/xfce4/xfconf/xfce-perchannel-xml/xfwm4.xml".text = ''
      <?xml version="1.0" encoding="UTF-8"?>
      <channel name="xfwm4" version="1.0">
        <property name="general" type="empty">
          <property name="use_compositing" type="bool" value="false"/>
        </property>
      </channel>
    '';
  };
  isoImage.squashfsCompression = "zstd -Xcompression-level 6";
  boot.kernelParams = [
    "console=ttyS0,115200n8"
    "console=tty0"
  ];
}
