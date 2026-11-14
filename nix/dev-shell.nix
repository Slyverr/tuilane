{
  mkShell,
  cargo,
  clippy,
  nixfmt,
  rust-analyzer,
  rustc,
  rustfmt,
  stdenv,
}:

mkShell {
  packages = [
    cargo
    clippy
    nixfmt
    rust-analyzer
    rustc
    rustfmt
    stdenv.cc
  ];
}
