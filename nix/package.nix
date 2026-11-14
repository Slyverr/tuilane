{
  lib,
  rustPlatform,
  sqlite,
}:

let
  workspace = builtins.fromTOML (builtins.readFile ../Cargo.toml);
  application = builtins.fromTOML (builtins.readFile ../crates/tuilane/Cargo.toml);
in
rustPlatform.buildRustPackage {
  pname = "tuilane";
  version = workspace.workspace.package.version;
  src = ../.;

  cargoLock.lockFile = ../Cargo.lock;
  cargoBuildFlags = [
    "--package"
    "tuilane"
  ];
  cargoTestFlags = [ "--workspace" ];
  buildInputs = [ sqlite ];

  meta = {
    description = application.package.description;
    license = with lib.licenses; [
      mit
      asl20
    ];
    mainProgram = "tuilane";
    platforms = lib.platforms.linux;
  };
}
