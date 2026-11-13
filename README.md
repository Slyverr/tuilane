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

The current build includes an interactive shell, Tasks, Calendar, Projects, Notes, Ideas, and Links views, plus CLI commands for those features and full-text search across tasks, projects, notes, and ideas. The shell command palette opens with `Ctrl-P` and can switch between registered views or run workspace actions. Tasks, projects, notes, ideas, and relations are stored in SQLite and use the shared validated host command path. Calendar supports month/date navigation and queries due tasks through the host service broker; its inspector refreshes after task changes and remains usable if the Tasks service is unavailable or has an incompatible version. Notes support Markdown editing through the configured external editor. Links support directed relations, backlinks, and file/URL references between validated first-party items. Portable SQLite backups are supported; loadable plugins are not implemented yet.

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
cargo run -p tuilane -- idea create --title "Connect project reviews with notes"
cargo run -p tuilane -- idea list
cargo run -p tuilane -- link add idea:IDEA_ID note:NOTE_ID
cargo run -p tuilane -- link list idea:IDEA_ID
cargo run -p tuilane -- link backlinks note:NOTE_ID
cargo run -p tuilane -- link add-reference idea:IDEA_ID --url https://example.org/research
cargo run -p tuilane -- backup create ./workspace-backup.sqlite3
cargo run -p tuilane -- backup restore ./workspace-backup.sqlite3 --yes
cargo run -p tuilane -- search "release notes"
cargo run -p tuilane -- search --kind note "release notes"
```

Backup creation does not overwrite an existing file. Restore replaces all current workspace data and requires `--yes`.

Task, project, note, idea, and relation data is stored in the platform's application data directory. Set `TUILANE_DATA_DIR` to use a different data directory. Link item references use the form `kind:UUID`, where the UUID comes from a task, project, note, or idea command. Replace uppercase `*_ID` placeholders in the examples with those returned IDs. Note creation and editing open the executable named by `VISUAL` or `EDITOR` (`VISUAL` takes precedence). For example, set `EDITOR=nvim` before running `cargo run -p tuilane -- note create --title "Research"` or `cargo run -p tuilane -- note edit NOTE_ID`. Set the variable to one executable path; it is invoked directly without a shell.

User-facing installation instructions will be added when installable releases become available.

## Development and contributing

Contributions, feedback, and suggestions are welcome as the project develops.

- [Contributing](CONTRIBUTING.md) — Development setup, contribution workflow, and quality requirements.
- [Roadmap](ROADMAP.md) — Planned capabilities and development milestones.
- [Architecture](docs/architecture.md) — System design and technical direction.

For AI-assisted development, see [AGENTS.md](AGENTS.md).

## License

tuilane is intended to be dual-licensed under MIT or Apache-2.0, at your option. See `LICENSE-MIT` and `LICENSE-APACHE` for the applicable terms.
