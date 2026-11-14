# tuilane

tuilane is a local-first terminal workspace for organizing tasks, projects, notes, ideas, and personal workflows. It brings a command-line interface and keyboard-driven terminal UI together with local storage, calendar, links, search, and an optional local assistant.

The project is in early development. Core workflows run locally, the assistant is optional, and a beta WebAssembly plugin SDK supports host-mediated commands and text views from independently built components. Remote providers and synchronization remain future work.

## Get started

Install Rust and a native linker. From the workspace root, run the app or its test suite:

```sh
cargo run -p tuilane
cargo test --workspace
```

For installation and Nix builds, see [installation](docs/installation.md). The [plugin SDK guide](docs/plugin-sdk.md) explains the current beta contract, while the [roadmap](ROADMAP.md) describes planned work. See [configuration](docs/configuration.md) for local settings and [CONTRIBUTING.md](CONTRIBUTING.md) to get involved.

## License

tuilane is dual-licensed under [MIT](LICENSE-MIT) or [Apache-2.0](LICENSE-APACHE), at your option. See those files for the license terms.
