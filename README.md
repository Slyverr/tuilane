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

The current build includes an interactive shell, Tasks, Calendar, Projects, and Notes views, plus CLI commands for those features. Tasks, projects, and notes are stored in SQLite and use the shared validated host command path. Calendar supports month/date navigation and queries due tasks through the host service broker; its inspector refreshes after task changes and remains usable if the Tasks service is unavailable or has an incompatible version. Notes support Markdown editing through the configured external editor. Ideas, cross-feature links, backups, and loadable plugins are not implemented yet.

For implementation progress and upcoming milestones, see the [roadmap](ROADMAP.md).

## Getting started

Use a compatible Rust toolchain and run the workspace in an interactive terminal:

```sh
cargo run -p tuilane
cargo run -p tuilane -- --help
cargo run -p tuilane -- --version
cargo run -p tuilane -- task create --title "Review documentation" --due 2026-11-20
cargo run -p tuilane -- task list
cargo run -p tuilane -- project create --title "Release planning"
cargo run -p tuilane -- project list
cargo run -p tuilane -- note list
cargo run -p tuilane -- note show NOTE_ID
```

Task, project, and note data is stored in the platform's application data directory. Set `TUILANE_DATA_DIR` to use a different data directory. Note creation and editing open the executable named by `VISUAL` or `EDITOR` (`VISUAL` takes precedence). For example, set `EDITOR=nvim` before running `cargo run -p tuilane -- note create --title "Research"` or `cargo run -p tuilane -- note edit NOTE_ID`. Set the variable to one executable path; it is invoked directly without a shell.

User-facing installation instructions will be added when installable releases become available.

## Development and contributing

Contributions, feedback, and suggestions are welcome as the project develops.

- [Contributing](CONTRIBUTING.md) — Development setup, contribution workflow, and quality requirements.
- [Roadmap](ROADMAP.md) — Planned capabilities and development milestones.
- [Architecture](docs/architecture.md) — System design and technical direction.

For AI-assisted development, see [AGENTS.md](AGENTS.md).

## License

tuilane is intended to be dual-licensed under MIT or Apache-2.0, at your option. See `LICENSE-MIT` and `LICENSE-APACHE` for the applicable terms.
