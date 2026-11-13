# tuilane roadmap

This file is the **authoritative index of planned product milestones**, not a list of implemented features or a live task checklist. Version numbers are **targets**, not guarantees or release dates. Actual work is tracked through GitHub Issues and Milestones; shipped changes are recorded in [CHANGELOG.md](CHANGELOG.md).

The initial focus is a useful, local-first task/project workspace with a composable terminal shell. Additional notes, ideas, calendar, AI and third-party plugins grow on the same host contracts without making a speculative framework the product.

## Tracking model

- **This file:** ordered chapters, intended version targets, dependencies, scope, measurable exit outcomes.
- **Detailed chapter specifications:** add one for an *active or near-term* chapter only when it needs more detail. Do not create a file for every hypothetical version by default.
- **GitHub Milestone `vX.Y.Z`:** issues/PRs for a targeted release and their actual progress. Create when that release is actively planned; no GitHub Projects board required initially.
- **GitHub Issue:** one actionable feature, defect, or infrastructure task. Tiny edits need not become separate issues.
- **Maintainer branch:** implementation and validation, then fast-forward to `main`. **External contributor branch:** submitted through a PR. **Tag:** exact released source.
- **`CHANGELOG.md`:** real user-facing changes, never plans.

Completion is proven by implementation, user-visible behavior, and passing acceptance criteria—not by writing documents or closing an arbitrary number of issues. A maintainer explicitly authorizes release/tagging under [docs/releasing.md](docs/releasing.md).

## Current state and naming

The current baseline is an eleven-member Cargo workspace with a composable terminal shell and first-party Tasks, Calendar, Projects, Notes, Ideas, and Links features in early development. Calendar queries due tasks through the host broker and refreshes after task changes. Projects support durable CRUD through the CLI and TUI. Notes support durable Markdown CRUD, external-editor workflows, stable item lookup, and a TUI view. Ideas support durable capture, lookup, rename, and removal through the CLI and TUI. Links support validated directed relations, backlinks, file/URL references, terminal navigation, and event-driven cleanup after item deletion. Inspect source, tests, CI, and tags before assessing milestone completion. CH-00 establishes the foundation; CH-01 is the first release target, `v0.1.0`. Never move/reuse an existing tag; adjust future version targets if necessary.

Legacy prototype behavior or data should only be ported through tracked, validated tasks after the relevant source/fixtures are available in the repository. Do not depend on private or untracked source locations.

## Milestone overview

| Chapter / target | Goal | User-observable exit condition |
| --- | --- | --- |
| CH-00 · foundation | Validated Cargo foundation | One self-contained five-crate workspace builds/tests; CLI help/version and CI verified; no fictional functionality |
| CH-01 · `v0.1.0` | Host kernel and composable TUI shell | Two registered views navigate correctly; inspector/overlay/focus/resize work |
| CH-02 · `v0.2.0` | SQLite vault, service broker, Tasks | Tasks CRUD via CLI/TUI survives restart; validated service query; one command path |
| CH-03 · `v0.3.0` | Calendar and plugin composition | Calendar reads dated tasks through broker, works with Tasks disabled |
| CH-04 · `v0.4.0` | Projects, Notes, Ideas and links | Idea ↔ note ↔ task ↔ project relations survive restart and edits |
| CH-05 · `v0.5.0` | Daily UX, search, personalization | Keyboard-driven capture, palette, filters, saved views, export/restore |
| CH-06 · `v0.6.0` | Agent-ready command contracts | Descriptors, scoped caller permissions, previews/confirmation, receipts, cancellation, and safe bulk operations; no LLM required |
| CH-07 · `v0.7.0` | Optional assistant and prompt UI | Assistant can propose approved operations; offline use unaffected |
| CH-08 · `v0.8.0` | Usable alpha and recovery | Clean install, reliable vault backup/restore, terminal fallback, migration checks |
| CH-09 · `v0.9.0` | External runtime feasibility | Measured Wasm/WIT isolation/performance spike with explicit go/no-go |
| CH-10 · `v0.10.0` | Third-party plugin SDK beta | Independent plugin loads, communicates, loses permission safely, and unloads |
| CH-11 · `v0.11.0` | Community beta and packaging | Verified install/upgrade, diagnostics, sample plugins and compatibility suite |
| CH-12 · `v0.12.0` | Public beta and real-world validation | Independent user feedback informs fixes, documentation, and subsequent development |

All targets are **proposed** until implementation evidence and the actual version/tag history establish otherwise.

## Near-term chapters

### CH-00 — Validated foundation

**Purpose:** make the existing scaffold a reproducible, well-documented basis for contributors and agents. Confirm workspace manifests, lockfile, crate boundaries, basic CLI behavior and Linux CI against the actual repo. Implement only necessary corrections and baseline tests; do not label a placeholder as a functional TUI.

**Exit:** fresh checkout builds and passes the documented checks, current CLI behaviors are tested, CI reports truthfully, and no tracked file relies on private paths or untracked data.

### CH-01 — Composable kernel and shell · v0.1.0

**Purpose:** prove host-side registration and UI contribution concepts before adding domain/storage complexity. Implement navigation and demo views, main/inspector/drawer/overlay slots, focus rules and terminal lifecycle.

**Exit:** a view can be registered or removed without editing a hardcoded central screen switch; keyboard, resize, missing contributions and terminal cleanup are validated.

## Subsequent chapters — scope and gates

### CH-02 · v0.2.0 — Vault, broker and Tasks

**Depends:** CH-01. Add real SQLite migrations, task models and a `plugins/tasks` crate; implement validated Tasks CRUD, CLI/TUI parity, `tasks.query.by_date@1` and service discovery through the host. **Verify:** restart durability, transaction/error handling, invalid/denied calls and no cross-plugin private access. **Exclude:** external plugin runtime and other domain modules.

### CH-03 · v0.3.0 — Calendar integration

**Depends:** CH-02. Calendar provides month/date navigation and an inspector contribution; queries Tasks through the host broker and updates after committed changes. **Verify:** missing/disabled Tasks, incompatible service version, refresh and no direct dependency on Tasks implementation. **Exclude:** calendar sync, meeting backend and advanced recurrence.

### CH-04 · v0.4.0 — Connected knowledge

**Depends:** CH-02/CH-03. Introduce Projects, Notes and Ideas feature crates; stable item IDs, cross-feature links, backlinks, file/URL references and Markdown via an external editor. **Verify:** create idea, attach note, link task/project, follow links, edit/rename and restart without orphaning relations. **Exclude:** full WYSIWYG and indiscriminate filesystem indexing.

### CH-05 · v0.5.0 — Everyday productivity

**Depends:** CH-04. Inbox/quick capture, command palette, FTS search, filters, saved views, keymaps/themes/layout presets, portable exports and basic backups. **Verify:** small terminal navigation, export/restore roundtrip, invalid configuration diagnostics. **Exclude:** arbitrary layout DSL, sync and marketplace.

### CH-06 · v0.6.0 — Agent-compatible commands

**Depends:** CH-02 and CH-05. Make commands discoverable with input/output/effect descriptions, caller identity, scoped permissions, previews/confirmation, receipts, cancellation and safe bulk operations. **Verify:** synthetic untrusted caller attempts permitted and denied actions without direct storage bypass. **Exclude:** an embedded LLM or autonomous background agent.

### CH-07 · candidate v0.7.0 — Optional assistant

**Depends:** CH-06 and CH-01 drawer/overlay. Optional model provider and UI contributions, scoped context, streaming/cancellation, and host-mediated approved tool calls. **Verify:** denial of unauthorized/destructive actions, opt-in use, and working offline without AI. **Exclude:** mandatory subscriptions or unrestricted unattended execution.

### CH-08 · candidate v0.8.0 — Alpha hardening

**Depends:** the usable flows from earlier chapters; the assistant may remain experimental. Improve install, diagnosis, backup/restore, terminal fallback, import/migration where real fixtures exist, accessibility/focus and error recovery. **Verify:** clean Linux install, roundtrip data, damaged-input handling, terminal cleanup. **Exclude:** stable external plugin ABI.

### CH-09 · candidate v0.9.0 — External plugin runtime evaluation

**Depends:** functional host contracts and permission tests. Prototype an isolated Wasm/WIT guest and benchmark startup, memory, call overhead, denial behavior, cancellation, failure recovery and authoring complexity; compare lightweight scripting only if justified. **Gate:** document a measured go/no-go. If inadequate, revise the next milestone instead of claiming plugin safety.

### CH-10 · candidate v0.10.0 — External plugin SDK beta

**Depends:** CH-09 passes. Implement versioned manifests, host services/UI contributions, install/enable/disable, grants/revocation, compatibility errors, and an independently authored sample extension. **Verify:** no host recompilation, permission denial, uninstall safety and recovery. **Exclude:** automatic marketplace.

### CH-11 · candidate v0.11.0 — Community beta

**Depends:** usable SDK, real feedback. Improve compatibility tests, diagnostics, sample plugins, supported release artifacts, checksums, docs and upgrade paths. **Verify:** clean external installation, version mismatch handling, preserved user data and reproducible samples. **Exclude:** unrestricted third-party distribution service.

### CH-12 · v0.12.0 — Public beta and real-world validation

**Depends:** CH-11 packaging and external SDK beta. Prepare tuilane for wider public testing. Gather feedback from actual users and contributors; exercise real-world workflows and plugin integrations; identify usability, compatibility, performance, and reliability problems; fix discovered issues and improve documentation. Establish a sustainable process for receiving, tracking, triaging, and following up on feedback and issues.

**Verify:** test supported workflows, plugin integrations, platforms, and installation methods; record and triage independent user feedback; address discovered high-impact issues and update affected documentation. Keep issue reporting and follow-up practical for maintainers and contributors. Internal tests and agent validation support this work but do not replace real-world feedback.

**Exclude:** declaring stable compatibility guarantees or treating this chapter's completion as qualification for `v1.0.0`.

## Future stability milestone — `v1.0.0`

`v1.0.0` is a separate future stability commitment, with no predefined release schedule or development chapter. Completing CH-12 or any other roadmap chapter does not automatically qualify tuilane for `v1.0.0`; roadmap completion alone does not prove production readiness. Continuing with `v0.14.0`, `v0.15.0`, and later `0.x` versions remains acceptable for as long as needed. Follow the readiness criteria in [docs/releasing.md](docs/releasing.md).

## Cross-chapter gates

1. **Architecture proof:** CH-03 demonstrates optional cross-plugin communication through the broker.
2. **Daily usefulness:** CH-05/08 show durable offline capture, retrieval, export, recovery.
3. **Agent safety:** CH-06 denies unauthorized and destructive requests under tests.
4. **Runtime isolation:** CH-09 has measurable negative tests before external SDK beta.
5. **Public beta learning:** CH-12 gathers independent user feedback and real workflow/plugin experience; this informs later `0.x` work and does not itself establish `v1.0.0` readiness.

## Maintenance rules

- Change chapter scope or acceptance only with maintainer approval; preserve decision rationale in a linked issue/ADR when significant.
- Add a detailed specification for an active milestone only when this roadmap lacks needed scope detail. Keep detailed engineering tasks in GitHub Issues, not duplicate checklists here.
- Do not mark a chapter complete or update user-facing README claims based on plans alone. Cross-check code, tests, issues, CI and release tags.
- When an implementation differs from the plan for a good reason, update the authoritative roadmap and architecture/ADR rather than accumulating contradictory descriptions.
- Completing an authorized chapter includes its target release commit and annotated tag after acceptance criteria and release checks pass; follow [docs/releasing.md](docs/releasing.md). A chapter never implies `v1.0.0` readiness.
