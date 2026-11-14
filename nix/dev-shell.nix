{
  mkShell,
  cargo,
  clippy,
  nixfmt,
  python3,
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
    python3
    rust-analyzer
    rustc
    rustfmt
    stdenv.cc
  ];
}
