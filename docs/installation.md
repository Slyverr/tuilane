# Installation

Tuilane can be built from source with Cargo or installed through the Nix flake.
For new version tags, CI is configured to produce a checksummed
`x86_64-unknown-linux-gnu` archive as a temporary GitHub Actions artifact.
These artifacts are not published GitHub Releases or package-manager
releases. The Nix flake defines packages for `x86_64-linux` and
`aarch64-linux`; its build has been verified on `x86_64-linux`.

## Linux release archive

For a new version tag, download its archive and checksum from the CI workflow
run's **Artifacts** section. The archive contains the executable, README,
changelog, build information, and both license files.
The binary targets x86_64 Linux with glibc; systems with an incompatible glibc
can use the Nix package or build from source.

Verify and install the archive, replacing only the executable during upgrades:

```sh
archive=tuilane-VERSION-x86_64-unknown-linux-gnu.tar.gz
sha256sum --check "$archive.sha256"
tar -xzf "$archive"
install -Dm755 "${archive%.tar.gz}/bin/tuilane" "$HOME/.local/bin/tuilane"
tuilane --version
tuilane doctor
```

Keep the application data directory when upgrading. The executable stores its
database and plugin state separately from the installation path. `tuilane
doctor` checks existing storage without modifying it.

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
