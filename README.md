# tuilane

tuilane is a local-first terminal workspace for organizing tasks, projects, notes, ideas, and personal workflows. It brings a command-line interface and keyboard-driven terminal UI together with local storage, calendar, links, search, and an optional local assistant.

The project is in early development. Core workflows run locally, and the assistant is optional. Remote providers, external plugins, and synchronization are future work.

## Get started

Install Rust and a native linker. From the workspace root, run the app or its test suite:

```sh
cargo run -p tuilane
cargo test --workspace
```

For installation and Nix builds, see [installation](docs/installation.md). The [configuration guide](docs/configuration.md) covers local settings, the [roadmap](ROADMAP.md) describes planned work, and [CONTRIBUTING.md](CONTRIBUTING.md) explains how to contribute.

## License

tuilane is dual-licensed under [MIT](LICENSE-MIT) or [Apache-2.0](LICENSE-APACHE), at your option. See those files for the license terms.
