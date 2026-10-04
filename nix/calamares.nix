{ pkgs }:
let
  pin = builtins.fromJSON (builtins.readFile ./calamares-source.json);
  source = pkgs.fetchFromGitHub pin;
in
import "${source}/nix/package.nix" { inherit pkgs; }
