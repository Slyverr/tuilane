# Architecture — tuilane

This document separates the **documented current baseline** from the **approved intended architecture**. A roadmap target or accepted design decision does not establish that code exists. Verify current modules and manifests in the checkout before changing them.

## Current baseline (verified in source)

The repository is an eleven-member Cargo workspace. The `tuilane` executable uses Clap for help, version, task, project, note, idea, and link commands; it composes the terminal shell, Tasks, Calendar, Projects, Notes, Ideas, and Links views. The shell uses Ratatui and Crossterm; non-interactive startup returns a clear error. Task, project, note, idea, and relation records use SQLite and persist across process restarts.

| Component | Ownership | Current implementation status |
| --- | --- | --- |
| `crates/tuilane` | CLI/TUI executable and composition root | Clap task/project/note/idea/link CLI; composes Tasks, Calendar, Projects, Notes, Ideas, Links, Overview, and Shortcuts views |
| `crates/tuilane-core` | Kernel, registration, broker, commands, events, policy | Ordered view registry, typed command dispatcher, versioned service broker, and synchronous typed event bus; policy is planned |
| `crates/tuilane-ui` | Terminal lifecycle, layouts, widgets, input/focus | Ratatui shell, responsive navigation, focus, inspector/drawer slots, help overlay, and managed terminal lifecycle |
| `crates/tuilane-storage` | SQLite adapter, migrations, backup/restore | File-backed and in-memory connections, WAL setup, and transactional access |
| `crates/tuilane-plugin-api` | Host-neutral contracts and contribution types | View metadata, typed command/service/event contracts, validated task/note types, stable item references, and typed relation/external-reference contracts |
| `plugins/tasks` | First-party Tasks feature | SQLite migration, task CRUD, due-date/item lookup providers, TUI contribution, and committed change/removal events |
| `plugins/calendar` | First-party Calendar feature | Month/date navigation and a Tasks inspector routed through the service broker; refreshes after task-change events and handles a missing or incompatible provider |
| `plugins/projects` | First-party Projects feature | SQLite-backed project CRUD, versioned item lookup service, and TUI contribution |
| `plugins/notes` | First-party Notes feature | SQLite-backed Markdown notes, versioned item lookup service, removal/change events, and TUI contribution; CLI editing uses `VISUAL` or `EDITOR` |
| `plugins/ideas` | First-party Ideas feature | SQLite-backed idea capture and rename/delete workflows, versioned item lookup service, removal/change events, and TUI contribution |
| `plugins/links` | First-party Links feature | SQLite-backed directed item relations, backlinks, file/URL references, service-validated endpoints, item-removal cleanup, and TUI contribution |
| Other `plugins/*` | Built-in domain feature crates | Planned, created as implementation requires |

The current manifests, source, tests, and CI configuration reflect these statuses. Update this section as implementation changes and tests verify additional behavior.

## Architectural invariants (approved design)

1. **Composition root:** `crates/tuilane` wires the kernel, UI, persistence adapter and built-in plugins. It owns argument parsing but not domain business rules.
2. **Core independence:** `tuilane-core` does not import terminal renderer, concrete SQLite implementation, or feature-plugin internals. Use ports/interfaces for capabilities it needs.
3. **Plugin API independence:** `tuilane-plugin-api` contains host-neutral service, command and UI contribution contracts; it must not depend on core, Ratatui, SQLite, or individual plugins.
4. **UI separation:** `tuilane-ui` owns terminal-specific rendering, navigation, focus, input arbitration and lifecycle. Plugin contributions use host-defined slots or public component contracts.
5. **Storage separation:** `tuilane-storage` owns backend adapter concerns. Domain plugins own business data and migrations; no plugin reads another plugin's private tables.
6. **Domain ownership:** Built-in tasks, calendar, projects, notes and ideas are distinct Rust feature crates when implemented. They communicate through registered contracts, not direct calls to another plugin's private Rust interfaces.
7. **One action path:** CLI, TUI and optional agent callers eventually use the same validated command/query dispatch path; no bypasses for convenience.
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

The following capabilities remain planned: lifecycle hooks, command schemas and caller identities, permission checks, configuration management, and external plugin loading. The current in-process dispatcher, service broker, and event bus route typed built-in contracts; they do not implement authorization, service-version negotiation, or isolation.

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

**Representation rule:** JSON-like examples explain the public mental model, not a requirement to serialize every in-process Rust call. A typed Rust dispatcher may execute trusted built-ins efficiently; a future Wasm adapter can translate the same logical contracts into a portable serialized interface. The exact schema language, runtime, registry and transport require implementation experiments.

**Current integration:** Calendar selects a date, queries `tasks.query.by_date@1` through the host when available, renders task summaries in its inspector, and refreshes after `tasks.changed`. It reports a missing or incompatible provider while keeping date navigation available. Calendar depends on the public plugin API and host contracts, not the Tasks implementation crate or its database.

### UI shell

The host owns sidebar/navigation, primary content region, contextual inspector, overlays/modals, bottom drawer, status feedback, responsive layout, focus and keyboard routing. Plugins supply bounded contributions. Start with simple supported layout presets, not unrestricted nested arbitrary panes. Ensure fallback under small terminal dimensions and limited capabilities.

### Local data and persistence

SQLite is the current local transactional store for Tasks, Projects, Notes, Ideas, and Links and the intended basis for other plugin-owned schemas, migration ownership, safe backups, and stable cross-feature identities/relations. Notes store bounded Markdown text; the CLI opens the user's configured editor using a private temporary file and persists content only after the editor exits successfully. The Links feature resolves item endpoints through each owner's typed service and removes incoming/outgoing relations and owned references after item-removal events. TOML configuration and filesystem attachments may be introduced when warranted. Cross-plugin access uses services rather than unmediated SQL. No network/cloud account is required for core workflows.

### Optional agents and external plugins

An AI assistant is an optional plugin/adapter, not a kernel dependency. It can discover approved commands, propose calls, and obtain confirmation for meaningful writes; it never bypasses the host command path. Agent output is untrusted input.

External plugin runtime candidates (such as Wasm Component Model/WIT) are deferred until first-party services prove the contracts. Runtime choice requires resource, isolation, cancellation, permission and authoring tests. Do not claim that native Rust ABI plugins offer isolation.

## Architecture ownership and evolution

- This document owns **current architecture and intended dependency rules**.
- Record significant design rationale in the relevant architecture section, including alternatives and consequences.
- [ROADMAP.md](../ROADMAP.md) owns *when* planned subsystems should become functional.
- [Coding standards](coding-standards.md) owns coding/testing rules.
- Discuss substantial architecture changes with maintainers before implementation, then update this document after approval and implementation.

Only label an area **implemented** after source inspection and relevant tests demonstrate it.
