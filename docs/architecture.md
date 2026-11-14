# Architecture — tuilane

This document separates the **documented current baseline** from the **approved intended architecture**. A roadmap target or accepted design decision does not establish that code exists. Verify current modules and manifests in the checkout before changing them.

## Current baseline (verified in source)

The repository is a fourteen-member Cargo workspace. The `tuilane` executable uses Clap for help, version, task, project, note, idea, link, backup, search, saved-search, plugin management, configuration-check, and read-only doctor commands; it composes the terminal shell, Tasks, Calendar, Projects, Notes, Ideas, Links, Search, Overview, and enabled external plugin views. When enabled in configuration, it also registers the Assistant view. The shell uses Ratatui and Crossterm; non-interactive startup returns a clear error. A versioned TOML file selects built-in shell theme, layout, and keymap presets. Task, project, note, idea, relation, and saved-search records use SQLite and persist across process restarts.

| Component | Ownership | Current implementation status |
| --- | --- | --- |
| `crates/tuilane` | CLI/TUI executable and composition root | Clap task/project/note/idea/link/backup/search/saved-search/plugin/config-check/doctor CLI; loads shell presets, mediates external plugin services, and composes built-in and enabled external views |
| `crates/tuilane-core` | Kernel, registration, broker, commands, events, policy | Ordered view registry, sorted operation catalog, typed command dispatcher with exact caller grants, prepared-command confirmation, and payload-free execution receipts; versioned service broker and synchronous typed event bus |
| `crates/tuilane-ui` | Terminal lifecycle, layouts, widgets, input/focus | Ratatui shell, responsive navigation, built-in theme/layout/keymap presets, focus, inspector/drawer slots, help and command-palette overlays with view-contributed actions, and managed terminal lifecycle |
| `crates/tuilane-storage` | SQLite adapter, migrations, backup/restore | File-backed and in-memory connections, WAL setup, transactional access, read-only database integrity and format inspection, validated standalone backup creation, and guarded restore with recovery copy |
| `crates/tuilane-plugin-api` | Host-neutral contracts and contribution types | View metadata, typed command/service/event contracts with operation IDs, input/output descriptions, declared effects, and scoped permissions; beta versioned Wasm manifest and WIT contracts; validated task/note types, stable item references, and typed relation/external-reference contracts |
| `crates/tuilane-plugin-runtime` | External WebAssembly component lifecycle | Versioned manifest validation, bundle installation, enable/disable/uninstall, persisted grants, bounded Wasmtime execution, host-service mediation, command dispatch, and validated text-view contributions |
| `plugins/tasks` | First-party Tasks feature | SQLite migration, task CRUD, bounded atomic completion of up to 100 unchanged task snapshots, due-date/item lookup providers, TUI contribution with command-palette quick creation, and committed change/removal events |
| `plugins/calendar` | First-party Calendar feature | Month/date navigation and a Tasks inspector routed through the service broker; refreshes after task-change events and handles a missing or incompatible provider |
| `plugins/projects` | First-party Projects feature | SQLite-backed project CRUD, versioned item lookup service, and TUI contribution with command-palette creation |
| `plugins/notes` | First-party Notes feature | SQLite-backed Markdown notes, versioned item lookup service, removal/change events, and TUI contribution with command-palette creation; CLI editing uses `VISUAL` or `EDITOR` |
| `plugins/ideas` | First-party Ideas feature | SQLite-backed idea capture and rename/delete workflows, versioned item lookup service, removal/change events, and TUI contribution with command-palette capture |
| `plugins/links` | First-party Links feature | SQLite-backed directed item relations, backlinks, file/URL references, service-validated endpoints, item-removal cleanup, and TUI contribution |
| `plugins/search` | First-party workspace search | SQLite FTS5 index refreshed from typed item-owner search services after first-party item changes; persistent named queries with optional kind filters; CLI management and TUI save/browse/run flows with kind filtering |
| `plugins/assistant` | Optional local assistant view and provider adapter | Disabled by default; streams from a configured loopback Ollama-compatible endpoint and exposes bounded task read/create/complete tools through the host command dispatcher; every task change requires preview and confirmation |
| Other `plugins/*` | Built-in domain feature crates | Planned, created as implementation requires |

The current manifests, source, tests, and CI configuration reflect these statuses. Update this section as implementation changes and tests verify additional behavior.

## Architectural invariants (approved design)

1. **Composition root:** `crates/tuilane` wires the kernel, UI, persistence adapter and built-in plugins. It owns argument parsing but not domain business rules.
2. **Core independence:** `tuilane-core` does not import terminal renderer, concrete SQLite implementation, or feature-plugin internals. Use ports/interfaces for capabilities it needs.
3. **Plugin API independence:** `tuilane-plugin-api` contains host-neutral service, command and UI contribution contracts; it must not depend on core, Ratatui, SQLite, or individual plugins.
4. **UI separation:** `tuilane-ui` owns terminal-specific rendering, navigation, focus, input arbitration and lifecycle. Plugin contributions use host-defined slots or public component contracts.
5. **Storage separation:** `tuilane-storage` owns backend adapter concerns. Domain plugins own business data and migrations; no plugin reads another plugin's private tables.
6. **Domain ownership:** Built-in tasks, calendar, projects, notes and ideas are distinct Rust feature crates when implemented. They communicate through registered contracts, not direct calls to another plugin's private Rust interfaces.
7. **One action path:** CLI, TUI and the optional assistant use the same validated command/query dispatch path; no bypasses for convenience.
8. **Plugin isolation honesty:** Statically linked Rust plugins are trusted code running in the host process; a broker is not a security sandbox. External/untrusted plugins require a verified isolation boundary before granting hostile-code security assurances.

See [coding standards](coding-standards.md), [ROADMAP.md](../ROADMAP.md), and [release policy](releasing.md) for related quality, milestone, and release rules. The architectural boundaries are summarized below.

### Intended dependency direction

```text
                 tuilane (binary / composition root)
                   /      |       |         \
                  v       v       v          v
                ui      core   storage    plugins/*
                  \       |       /          /
                   \      v      /          /
                    tuilane-plugin-api

core    -/-> UI / concrete storage / individual plugins
pluginA -/-> pluginB internals (use public host services)
```

This diagram expresses **allowed ownership and prohibited reverse dependencies**, not a precise manifest dependency graph. Concrete adapters may depend on core ports as justified; no dependency cycle is allowed. Document exceptions before introduction.

## Future host capabilities

Command handlers currently declare stable operation-scoped permissions, and `CommandDispatcher::execute_as` checks the exact grant for a caller assigned by trusted host code. Read-only commands can execute directly. Writes and destructive operations require `preview_as` followed by `confirm`; the prepared value holds the exact typed request, confirmation rechecks the grant, and the request details are escaped before display. Authorized handler attempts return payload-free receipts with a dispatcher-local sequence, caller, operation, effect, and success/failure/cancelled outcome. Preflight denials and cancellation before dispatch do not receive receipts. Handlers registered with a command context can observe cancellation and acknowledge it when stopping at a safe point; the dispatcher preserves completed results when a handler does not acknowledge cancellation. Cancellation does not roll back side effects already made. Task bulk completion accepts up to 100 open-task snapshots, includes their details in the preview, checks snapshots again during confirmation, and commits the full batch atomically or makes no changes. The core cannot know whether a human approved the preview, so an adapter must keep that decision at a trusted host boundary. The existing `execute` entry point is reserved for trusted first-party in-process code. The dispatcher does not authenticate callers, and neither entry point isolates statically linked code. Untrusted adapters must use the authorized path and must never expose `execute` or `confirm` directly to an autonomous caller.

Registered commands expose stable, versioned operation IDs, short input/output descriptions, declared effects, and required permissions in deterministic ID order. These logical descriptions are discovery metadata rather than a wire format or runtime validator; adapters must validate input into the typed command contracts. Built-in services and events remain typed in-process contracts. The external plugin runtime translates a small versioned WIT interface into host-mediated services; its current API and service set are beta and do not establish stable compatibility guarantees.

The host controls lifecycle, registration, capability discovery, service dispatch, input/output validation, permission checks, event delivery, navigation, and shared UI state. Plugins register contributions rather than controlling the terminal event loop.

```text
Keyboard/TUI         CLI scripts          Optional agent/tool adapter
     |                    |                            |
     v                    v                            v
  TUI shell          CLI parser                    Scoped caller
     \                    |                            /
      +-------------------+---------------------------+
                          |
                   Host dispatcher
        services / commands / query / events
         context / lifecycle / authorization
                          |
            +-------------+--------------+
            |             |              |
        Tasks plugin   Calendar       Other plugins
            |             |              |
            +-------------+--------------+
                          |
              Storage ports -> SQLite
```

### Plugin service protocol

The desired contract resembles structured tool/function calling rather than Rust-to-Rust private imports. A provider advertises a **namespaced capability**, a contract version, input/output schemas, declared effects, errors and access requirements. A consumer discovers a compatible provider and sends a structured request **through the host**. The host resolves provider selection, validates data, applies caller permissions, mediates execution, and returns a typed/structured response.

Illustrative, **not a finalized wire format**:

```json
{
  "service": "tasks.query.by_date",
  "version": 1,
  "input": { "date": "2026-11-13" }
}
```

- Service calls request authoritative data or explicitly described operations.
- Commands represent validated user-visible operations and state mutations.
- Events announce completed changes; they are not an authoritative data store or a command transport.
- UI context describes current selection with bounded information, not unrestricted private record access.
- UI contributions register views, inspector cards, overlays, drawers and contextual actions inside host-owned slots.

**Contract rules:** deterministic resolution of multiple providers; missing optional providers handled gracefully; version negotiation; validation at boundaries; distinguish unavailable/invalid/denied/timed-out/provider-failure outcomes; bounded resource usage; invalidation on unload; scoped telemetry without private payloads. The current broker resolves exact capability identifiers and typed requests; event subscriptions end when their handles are dropped. Caller permissions are checked at invocation, not inferred from discovery.

**Representation rule:** JSON-like examples explain the public mental model, not a requirement to serialize every in-process Rust call. A typed Rust dispatcher executes trusted built-ins efficiently. The beta Wasm adapter uses versioned WIT and bounded JSON request/response strings for its currently supported host services; this contract may change during `0.x` development.

**Current integration:** Calendar selects a date, queries `tasks.query.by_date@1` through the host when available, renders task summaries in its inspector, and refreshes after `tasks.changed`. It reports a missing or incompatible provider while keeping date navigation available. External components can request `workspace.info@1` and `workspace.search@1` after the user grants each manifest-declared permission. Calendar depends on the public plugin API and host contracts, not the Tasks implementation crate or its database.

### UI shell

The host owns sidebar/navigation, primary content region, contextual inspector, overlays/modals, bottom drawer, status feedback, responsive layout, focus and keyboard routing. Plugins supply bounded contributions. Start with simple supported layout presets, not unrestricted nested arbitrary panes. Ensure fallback under small terminal dimensions and limited capabilities.

### Local data and persistence

SQLite is the current local transactional store for Tasks, Projects, Notes, Ideas, and Links and the intended basis for other plugin-owned schemas, migration ownership, safe backups, and stable cross-feature identities/relations. The storage adapter creates standalone SQLite backups without overwriting existing files and validates database integrity and format metadata before restoring; restore requires explicit CLI confirmation and keeps a recovery copy while SQLite applies it. Notes store bounded Markdown text; the CLI opens the user's configured editor using a private temporary file and persists content only after the editor exits successfully. The Links feature resolves item endpoints through each owner's typed service and removes incoming/outgoing relations and owned references after item-removal events. The executable reads a versioned TOML configuration for shell presets and the opt-in assistant; settings for other plugins and filesystem attachments remain future work. Cross-plugin access uses services rather than unmediated SQL. Core workflows require no network or cloud account.

### Optional assistant and external plugins

The built-in assistant is disabled by default and is not a kernel dependency. When enabled, it sends prompts and at most 20 open-task summaries to a local HTTP loopback endpoint using the Ollama chat protocol. The adapter disables proxies and redirects, runs streaming requests on a cancellable worker thread, and treats provider output as untrusted input. It exposes only task listing, creation, and completion; all task changes route through the host dispatcher and display a preview for explicit approval. Task deletion is unavailable. Conversation history is held only in memory. Workflows outside the assistant remain available without a running model service.

CH-09 evaluated Wasmtime 49.0.5 with WIT in an isolated experiment; its measured startup, call overhead, memory limit, permission denial, interruption, and recovery results supported the CH-10 SDK prototype. The experiment is documented in [the runtime evaluation](../experiments/runtime-evaluation/README.md). The application now loads versioned components through `tuilane-plugin-runtime`, with no WASI filesystem or network imports, bounded execution, explicit host services, and host-rendered text views. These controls have been tested with the included sample only; do not claim general sandbox security or a stable plugin API. A trapped or interrupted instance is discarded and recreated. Do not claim that native Rust ABI plugins offer isolation. See the [plugin SDK guide](plugin-sdk.md) for the supported beta contract and its current limits.

## Architecture ownership and evolution

- This document owns **current architecture and intended dependency rules**.
- Record significant design rationale in the relevant architecture section, including alternatives and consequences.
- [ROADMAP.md](../ROADMAP.md) owns *when* planned subsystems should become functional.
- [Coding standards](coding-standards.md) owns coding/testing rules.
- Discuss substantial architecture changes with maintainers before implementation, then update this document after approval and implementation.

Only label an area **implemented** after source inspection and relevant tests demonstrate it.
