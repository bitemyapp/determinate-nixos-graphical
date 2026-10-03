{ pkgs }:
pkgs.rustPlatform.buildRustPackage {
  pname = "respin-tools";
  version = "0.1.0";
  src = pkgs.lib.fileset.toSource {
    root = ../rust;
    fileset = pkgs.lib.fileset.unions [
      ../rust/Cargo.toml
      ../rust/Cargo.lock
      ../rust/src
    ];
  };
  cargoLock.lockFile = ../rust/Cargo.lock;
  buildFeatures = [ "calamares" ];
  nativeBuildInputs = [ pkgs.python3 ];
  buildInputs = [ pkgs.python3 ];
  PYO3_PYTHON = "${pkgs.python3}/bin/python3";
  # Unit tests include a local Unix socket mock for QMP/QGA; no network/VM access.
  doCheck = true;
  meta = {
    description = "RustScript implementations for the Determinate graphical respin";
    license = pkgs.lib.licenses.asl20;
    platforms = [ "x86_64-linux" ];
    mainProgram = "respin-tools";
  };
}
