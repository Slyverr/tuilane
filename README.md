# tuilane

**A local-first terminal workspace for everyday work.**

tuilane brings a keyboard-driven terminal interface and command-line tools together for organizing tasks, projects, notes, ideas, and related information.

## Status

tuilane is in early development. The current workspace includes built-in task, project, note, idea, calendar, link, and search features backed by local storage. External plugins, synchronization, and AI integrations are not implemented.

## Get started

Install a Rust toolchain with `rustfmt`, `clippy`, and a native linker, then run:

```sh
cargo run -p tuilane
cargo run -p tuilane -- --help
```

For a workspace check and tests:

```sh
cargo check --workspace
cargo test --workspace
```

## Project information

- [Contributing](CONTRIBUTING.md) — development setup and contribution workflow.
- [Roadmap](ROADMAP.md) — planned milestones and their acceptance criteria.
- [Architecture](docs/architecture.md) — crate responsibilities and system design.
- [Configuration](docs/configuration.md) — supported shell settings.

## License

tuilane is dual-licensed under [MIT](LICENSE-MIT) or [Apache-2.0](LICENSE-APACHE), at your option.
