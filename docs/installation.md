# Installation

Tuilane is currently built from source; release binaries and package-manager
releases are not yet provided. The flake defines Nix builds for `x86_64-linux`
and `aarch64-linux` (verified here on `x86_64-linux`). Cargo builds work on
systems with a supported Rust toolchain and native linker.

## Nix

From the repository root, enter the development shell and run the workspace:

```sh
nix develop
cargo run -p tuilane
```

Build the default package and run the resulting executable:

```sh
nix build
./result/bin/tuilane --help
```

To add the package to a Nix profile while working from the repository root:

```sh
nix profile add .#tuilane
tuilane --version
```

The flake pins Nixpkgs in `flake.lock`, so builds and development tools use the
same recorded Nixpkgs revision. `nix build` evaluates the package from the
current repository source; a clean checkout includes all files tracked by Git.

## Diagnose problems

Run `tuilane doctor` to validate the active configuration and check local
database integrity and format version. The check opens existing storage
read-only and reports an uninitialized workspace without creating its database.

## Cargo

Install the stable Rust toolchain selected by [`rust-toolchain.toml`](../rust-toolchain.toml),
including `rustfmt` and `clippy`, and a native linker. From the repository root:

```sh
cargo install --path crates/tuilane --locked
```

For local development, `cargo run -p tuilane` starts the application and
`cargo test --workspace` runs the workspace tests. See [CONTRIBUTING.md](../CONTRIBUTING.md)
for the complete validation commands.
