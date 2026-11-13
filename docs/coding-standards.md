# Engineering standards

This is the **authoritative technical quality policy** for tuilane. See [CONTRIBUTING.md](../CONTRIBUTING.md) for Git/PR workflow, [architecture.md](architecture.md) for layer ownership, and [releasing.md](releasing.md) for releases. These are project rules, not claims that every planned system already exists.

## Principles and scope

1. **Correctness and safety first.** Reject invalid input explicitly; preserve data and fail predictably.
2. **Simple before speculative.** Use clear Rust and the smallest maintainable abstraction that handles known requirements. Reuse existing sound patterns.
3. **Real boundaries.** A crate or plugin owns a defined responsibility; callers use its supported public contracts.
4. **Measurable improvement.** Refactor duplication, performance bottlenecks, or design defects when demonstrated. Do not turn small changes into unrelated rewrites.
5. **Portable, local-first behavior.** Linux is first-class; avoid needless assumptions that prevent macOS/Windows support.
6. **No unverified claims.** Tests, benchmarks, CI, packaging, and architecture documents must describe what was actually exercised or implemented.

For major design changes, obtain maintainer approval and record the rationale in [architecture documentation](architecture.md) where warranted.

## Rust style and structure

- Follow Rust API conventions, `rustfmt`, workspace lint policy, and useful Clippy diagnostics. Do not invent conflicting manual formatting rules.
- Prefer explicit, small modules organized around ownership and cohesive responsibilities. Make things private unless a consumer needs a supported public API.
- Use expressive types and meaningful names. Avoid redundant wrappers, unnecessary traits, overgeneralized generics, and global mutable state.
- Avoid duplicating logic between CLI, TUI, and plugin entry points. User-facing mutations should flow through one validated command path when that subsystem exists.
- Use `Result` for recoverable errors. Avoid `unwrap`/`expect` on files, user input, network responses, database results, terminal capabilities, or plugin data. Invariants that truly cannot fail may use `expect` with justification; tests may use it freely.
- Preserve error context at module boundaries. Use typed error variants for reusable libraries/contracts and human-friendly diagnostics at CLI/UI boundaries; do not mandate a specific error crate.
- Do not suppress lints, warnings, or test failures without a documented, narrowly scoped reason.
- Keep synchronous blocking operations out of latency-sensitive terminal rendering paths; use bounded work, cancellation, and explicit failure handling where applicable.
- Changes to serialized formats, SQLite schema, public enums, service IDs, and CLI flags need compatibility analysis and tests.

### Public API policy

- A public API exists only where a real consumer or stable integration requires it. Do not publish an internal type solely to make testing easier.
- Document public contracts: ownership, input/output semantics, important invariants, side effects, failures, and supported version range.
- Use stable namespaced identifiers and explicit version semantics for cross-plugin operations (see [architecture](architecture.md)); JSON-like schemas are a **logical contract proposal**, not a mandatory in-process serialization choice.
- Internal APIs and plugin contracts remain experimental during `0.x`, unless an explicit stability guarantee is documented. Breaking changes still require accurate migration notes.
- Prefer error results and capability detection to panics and silent fallbacks when plugins or commands are unavailable.

## Dependency policy

An agent may add a dependency **without per-dependency approval** if it is justified, maintained, compatible with project licensing/security requirements, and meaningfully better than a small local implementation or an existing dependency. In the change description, explain important additions and alternatives considered when consequential.

- Check maintenance, public API, supported Rust version, license, feature set, transitive footprint, and risk. Do not assume popularity alone makes a crate safe.
- Prefer workspace-shared dependencies when multiple members need the same dependency and compatible options.
- Use narrowly enabled features where practical; avoid unnecessary runtime/async frameworks and heavyweight stacks before a demonstrated use.
- Audit security-sensitive dependencies and review advisories before releases. Treat findings based on exploitability and actual dependency use; don't suppress issues without rationale.
- Do not add a new native plugin ABI, network service, scripting engine, or AI provider dependency merely to anticipate future requirements.

## Testing strategy

**Prefer black-box tests of public behavior.** Each crate's `tests/` directory is the default home for integration tests that use the public API as consumers would. Use `src/` colocated `#[cfg(test)]` unit tests for private invariants and logic that cannot be tested meaningfully through public behavior; do not make internals public just for tests.

| Test layer | Where | Covers |
| --- | --- | --- |
| Public integration/contract | `crates/<crate>/tests/` or `plugins/<plugin>/tests/` | Public methods, CLI-visible commands, service compatibility, failures |
| Private unit | Colocated `#[cfg(test)]` module | Non-public algorithms and invariants where justified |
| Cross-crate/integration | Appropriate host/binary crate `tests/` | Registration, command routing, persistence, module lifecycle |
| End-to-end | Binary/terminal test harness when available | Real CLI/TUI workflows, startup, focus, errors, restoration |

Requirements:

- Test observable success, invalid input, state transitions, error handling, and regression cases appropriate to the change.
- For storage, test migration/rollback, restart durability, transactional failure, and import/export roundtrips when implemented.
- For plugin services, test missing providers, version mismatches, invalid schemas, permission denial, cancellation, failures, and no private access as applicable.
- For terminal behavior, test small dimensions, focus ownership, overlays, terminal cleanup, and non-TTY behavior where feasible.
- Prefer deterministic fixtures and temporary directories over depending on a user's filesystem, locale, credentials, or network state.
- Tests should not introduce network requirements for ordinary local development. Isolate genuine external integration tests.
- Coverage percentages are **not** an initial acceptance gate; test quality and failure-mode coverage matter more. Introduce thresholds only with measured need.
- Never mark a test passing when it was skipped, not run, or could not exercise the intended behavior.

## Security and data handling

- Treat user files, imported content, plugin output, model instructions, and remote responses as untrusted inputs.
- Validate at the host/consumer boundary, not only in UI forms. Avoid executing generated shell commands or plugin actions based solely on model text.
- Maintain least privilege and scoped grants for external plugins and tools when implemented. A trusted, statically linked Rust plugin shares process privileges; service mediation is an architectural rule, **not a sandbox**.
- Do not log secrets or full private records by default. Redact diagnostic context appropriately; store credentials only through an explicitly designed secret-handling mechanism when needed.
- Deny unknown operations and unsupported versions predictably. Avoid authorization bypasses through CLI, TUI, or agent adapters.
- When persistence exists, respect schema ownership, use transactions for mutations, provide safe migrations, and never silently delete, corrupt, or abandon user data.
- Do not add mandatory telemetry, remote analytics, or model calls to an offline-first workflow.
- Security-relevant changes require focused negative tests and maintainer review of the affected trust boundary.

## Performance and portability

- Measure before optimizing. Prioritize perceived responsiveness, bounded memory, predictable startup, and nonblocking TUI interaction.
- Linux is the initial development/validation target; avoid platform-specific APIs without an adapter or explicit supported-platform limitation.
- Make Unicode/color/icon/graphics enhancements optional. Baseline terminal functionality must degrade gracefully when capabilities are absent.
- Use platform path conventions and honor user/environment configuration rather than hardcoding absolute paths.
- No benchmark or cross-platform compatibility claim without results from the relevant environment.

## Documentation change triggers

Update the document that owns the changed fact:

| Change | Document |
| --- | --- |
| CLI behavior, installation/status | `README.md` |
| Crate ownership, control flow, service architecture | `docs/architecture.md` |
| Consequential irreversible design choice | Rationale in `docs/architecture.md` |
| Engineering convention/test expectation | `docs/coding-standards.md` |
| Contributor/commit/PR workflow | `CONTRIBUTING.md` |
| Milestone objective, scope, gate | `ROADMAP.md` / detailed specification when needed |
| Released user-facing behavior | `CHANGELOG.md` / release notes |
| Publishing, tag, version procedure | `docs/releasing.md` |
| Agent operating rules | `AGENTS.md` |

Do not update the changelog for every internal commit; add meaningful unreleased entries as features become real. Do not treat roadmap text as evidence of implemented functionality.

## Definition of Done (changes)

A change is complete when:

1. The task's actual requirements and acceptance conditions are satisfied.
2. Critical failure cases and relevant public behavior are tested; no data-loss or privilege regression is introduced.
3. Applicable targeted and workspace checks pass, or exact blockers are reported.
4. Existing crate boundaries and supported contracts remain intact, or an approved migration is implemented.
5. Relevant documentation is updated and examples are accurate.
6. The complete diff has been reviewed for scope drift, sensitive data, untracked dependencies, and portability problems.
7. The agent/contributor reports what changed, what was validated, and what remains unresolved.

**Releases** have additional criteria defined in [docs/releasing.md](releasing.md). A passing build does not by itself complete a milestone.
