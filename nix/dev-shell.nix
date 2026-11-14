{
  mkShell,
  cargo,
  clippy,
  nixfmt,
  rust-analyzer,
  rustc,
  rustfmt,
}:

mkShell {
  packages = [
    cargo
    clippy
    nixfmt
    rust-analyzer
    rustc
    rustfmt
  ];
}
