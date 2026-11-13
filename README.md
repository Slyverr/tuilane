# tuilane

**A local-first, extensible terminal workspace.**

tuilane is an open-source Rust project aiming to bring tasks, projects, notes, ideas, and personal workflows into one keyboard-driven workspace.

It combines a terminal user interface (TUI) with a scriptable command-line interface (CLI), allowing different tools and plugins to work together without tightly coupling their implementations.

## Vision

- **One workspace:** Organize everyday work without switching between unrelated terminal applications.
- **Local-first:** Your data stays on your machine, without requiring cloud services or accounts.
- **Extensible:** Built-in and third-party plugins contribute functionality through shared, host-managed contracts.
- **Keyboard-driven:** Fast navigation, commands, and workflows designed for the terminal.
- **Modular:** Keep features independent, maintainable, and replaceable.

## Project status

**Early development.**

The project is currently building its foundation. The interactive workspace, persistence, feature plugins, and external plugin system are planned capabilities rather than completed releases.

For implementation progress and upcoming milestones, see the [roadmap](ROADMAP.md).

## Getting started

For the current development scaffold, use a compatible Rust toolchain and run:

```sh
cargo run -p tuilane -- --help
cargo run -p tuilane -- --version
```

User-facing installation instructions will be added when installable releases become available.

## Development and contributing

Contributions, feedback, and suggestions are welcome as the project develops.

- [Contributing](CONTRIBUTING.md) — Development setup, contribution workflow, and quality requirements.
- [Roadmap](ROADMAP.md) — Planned capabilities and development milestones.
- [Architecture](docs/architecture.md) — System design and technical direction.

For AI-assisted development, see [AGENTS.md](AGENTS.md).

## License

tuilane is intended to be dual-licensed under MIT or Apache-2.0, at your option. See `LICENSE-MIT` and `LICENSE-APACHE` for the applicable terms.
