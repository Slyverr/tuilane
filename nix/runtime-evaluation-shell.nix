{
  coreutils,
  gnugrep,
  gnused,
  mkShell,
  rustup,
  wasm-tools,
  stdenv,
}:

mkShell {
  packages = [
    coreutils
    gnugrep
    gnused
    rustup
    wasm-tools
    stdenv.cc
  ];
}
