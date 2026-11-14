# tuilane

Standalone tuilane archive for x86_64 Linux. The executable requires a
compatible glibc runtime.

## Install

Download the `.tar.gz` archive and its `.sha256` file from the version-tag
workflow artifact, verify it, extract it, and install the executable:

```sh
archive=tuilane-VERSION-x86_64-unknown-linux-gnu.tar.gz
sha256sum --check "$archive.sha256"
tar -xzf "$archive"
bundle=${archive%.tar.gz}
install -Dm755 "$bundle/bin/tuilane" "$HOME/.local/bin/tuilane"
tuilane --version
```

Replace `VERSION` with the version in the downloaded archive. Add
`$HOME/.local/bin` to `PATH` if needed.

## Update and data

Install a new executable at the same path to update tuilane. Its local
database, configuration, and plugin state are stored separately from the
executable; keep those files when upgrading. Run `tuilane doctor` to inspect
configuration and database health.

## License

tuilane is available under either the MIT License or Apache License 2.0. The
full license texts are included as `LICENSE-MIT` and `LICENSE-APACHE`.
