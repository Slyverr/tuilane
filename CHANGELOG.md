# Changelog

All notable **user-visible** changes to released versions of tuilane will be recorded here, following [Keep a Changelog](https://keepachangelog.com/en/1.1.0/) and [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Changed

- Add clearer workspace navigation icons and key hints, with `j`/`k` reserved for switching views and arrow keys for in-view lists.

### Fixed

- Allow `k` and `j` to reach workspace navigation from Notes, Ideas, and Links while browsing their lists.

## [0.10.0] - 2026-11-14

### Added

- Add a beta WebAssembly plugin SDK with versioned manifests, host-mediated services, declared commands and views, explicit grants, lifecycle management, and an independently built sample.

## [0.9.0] - 2026-11-14

No user-facing application behavior changed. This source release records the
CH-09 Wasm/WIT feasibility decision; the product still does not load external
plugins.

## [0.8.0] - 2026-11-14

- Add a pinned Nix flake for Linux builds, profile installation, and development.
- Enable `j`/`k` alongside arrow keys for workspace view navigation.
- Add read-only `tuilane doctor` checks for configuration and local database health.

## [0.7.0] - 2026-11-14

- Add an opt-in local assistant with streaming responses and reviewed task creation or completion.

## [0.6.0] - 2026-11-13

- Add host-mediated command descriptors, scoped permissions, previews and confirmation, payload-free execution receipts, and cooperative cancellation.
- Add atomic bulk task completion for up to 100 unchanged task snapshots.

## [0.5.0] - 2026-11-13

- Add command-palette quick-create actions, full-text search, item-kind filters, and persistent saved search views.
- Add standalone SQLite backup creation and confirmed restore.
- Add versioned TOML shell presets for themes, layouts, and keymaps with CLI validation.

## [0.4.0] - 2026-11-13

- Add durable Projects, Markdown Notes, and Ideas workflows through the CLI and TUI.
- Add directed item relations, backlinks, and file or URL references with cleanup after item deletion.

## [0.3.0] - 2026-11-13

- Add a navigable Calendar view that queries due tasks through the host broker and refreshes after task changes.

## [0.2.0] - 2026-11-13

- Add SQLite-backed task CRUD through the CLI and TUI with a shared command path and versioned due-date query service.

## [0.1.0] - 2026-11-13

- Add the initial five-crate workspace and runnable CLI foundation.
- Add host-managed view registration and a responsive terminal shell with keyboard navigation, overlays, and focus handling.

<!-- Add dated version sections only for versions that have actually been released. Do not copy planned ROADMAP.md entries into this file. -->
