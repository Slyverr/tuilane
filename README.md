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

The current build includes an interactive shell, a local Tasks view, and CLI task commands. Tasks are stored in SQLite and support create, list, show, update, complete, reopen, and delete operations through the same validated host command path. Calendar month navigation is under development but is not yet composed into the application or connected to Tasks. Projects, notes, ideas, backups, and loadable plugins are not implemented yet.

For implementation progress and upcoming milestones, see the [roadmap](ROADMAP.md).

## Getting started

Use a compatible Rust toolchain and run the workspace in an interactive terminal:

```sh
cargo run -p tuilane
cargo run -p tuilane -- --help
cargo run -p tuilane -- --version
cargo run -p tuilane -- task create --title "Review documentation" --due 2026-11-20
cargo run -p tuilane -- task list
```

Task data is stored in the platform's application data directory. Set `TUILANE_DATA_DIR` to use a different data directory.

User-facing installation instructions will be added when installable releases become available.

## Development and contributing

Contributions, feedback, and suggestions are welcome as the project develops.

- [Contributing](CONTRIBUTING.md) — Development setup, contribution workflow, and quality requirements.
- [Roadmap](ROADMAP.md) — Planned capabilities and development milestones.
- [Architecture](docs/architecture.md) — System design and technical direction.

For AI-assisted development, see [AGENTS.md](AGENTS.md).

## License

tuilane is intended to be dual-licensed under MIT or Apache-2.0, at your option. See `LICENSE-MIT` and `LICENSE-APACHE` for the applicable terms.
