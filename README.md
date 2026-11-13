# tuilane

**A local-first, modular terminal workspace for organizing tasks, projects, notes, ideas, and personal workflows.**

tuilane pairs a command-line interface and keyboard-driven terminal UI with local storage. The early-development workspace includes tasks, projects, notes, ideas, calendar, links, and search; external plugins, synchronization, and AI integrations are future work.

## Build and run

Install the Rust toolchain and a native linker, then run from the workspace root:

```sh
cargo run -p tuilane
cargo test --workspace
```

See [architecture](docs/architecture.md), [roadmap](ROADMAP.md), and [contributing](CONTRIBUTING.md) for project design, planned milestones, and development setup.

## License

tuilane is dual-licensed under [MIT](LICENSE-MIT) or [Apache-2.0](LICENSE-APACHE), at your option.
