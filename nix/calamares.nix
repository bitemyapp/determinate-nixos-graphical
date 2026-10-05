# The installer package, built from the locked `calamares` flake input.
{ pkgs, source }:
let
  pin = builtins.fromJSON (builtins.readFile ./calamares-source.json);
  # Development builds only: `RESPIN_CALAMARES_SRC=/abs/checkout nix build --impure`
  # builds the installer from a local checkout. Installed systems, reference
  # systems and tests still use the locked input's modules.
  local = builtins.getEnv "RESPIN_CALAMARES_SRC";
  installerSource =
    if local == "" then
      # Tools and the flasher read calamares-source.json; it must name the lock.
      assert source.rev == pin.rev && source.narHash == pin.hash;
      source
    else
      builtins.path {
        path = local;
        name = "source";
        filter =
          path: type:
          !(builtins.elem (baseNameOf path) [
            ".git"
            "target"
            "result"
          ]);
      };
in
import "${installerSource}/nix/package.nix" { inherit pkgs; }
