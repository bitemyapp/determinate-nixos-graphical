{ pkgs }:
let
  pin = builtins.fromJSON (builtins.readFile ./calamares-source.json);
  # Development builds only: `RESPIN_CALAMARES_SRC=/abs/checkout nix build --impure`.
  # Pure evaluation always sees an empty value and uses the pinned revision.
  local = builtins.getEnv "RESPIN_CALAMARES_SRC";
  source =
    if local == "" then
      pkgs.fetchFromGitHub pin
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
(import "${source}/nix/package.nix" { inherit pkgs; }).overrideAttrs (old: {
  # Lets checks and the ISO evaluate the same static NixOS modules.
  passthru = (old.passthru or { }) // {
    root = source;
  };
})
