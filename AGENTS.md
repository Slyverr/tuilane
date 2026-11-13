# Instructions for coding agents

These requirements apply to work in this repository. They supplement, not duplicate, the authoritative human-facing policies linked below. If instructions conflict, follow the more specific applicable policy and surface the conflict before making an incompatible change.

## Read the right context

1. Read the assigned task and inspect the relevant implementation, manifests, tests, and `git status` before editing. Never assume the roadmap is implemented.
2. Follow [engineering standards](docs/coding-standards.md) for Rust design, testing, safety, and validation, and [contributor workflow](CONTRIBUTING.md) for Git and commits.
3. Consult [architecture](docs/architecture.md) for crate boundaries, plugin contracts, persistence, or interface changes. Propose material architectural changes before implementation and record accepted rationale in the owning documentation.
4. For chapter work, consult [ROADMAP.md](ROADMAP.md) and any active milestone specification present in the repository. For releases, follow [docs/releasing.md](docs/releasing.md) instead of inventing a process.
5. Read additional docs only when relevant; do not load the entire repository's documentation for trivial changes.

## Execution rules

- Work from the explicitly assigned issue, chapter, objective, or maintainer feedback. Plan dependent substeps as needed; do not self-assign unrelated roadmap work.
- Preserve existing behavior and public contracts unless change is authorized. Respect ownership and allowed dependency direction in `docs/architecture.md`.
- Choose the simplest correct, idiomatic, maintainable solution. Reuse existing patterns when sound; refactor demonstrable duplication or weak designs **within a reasonable task scope**. Do not make speculative frameworks or broad rewrites.
- Propose **material architectural changes** before implementing them: new runtimes, major dependency direction changes, new persisted contract formats, trust/security model changes, or incompatible public interfaces. Small internal choices and local refactors are autonomous.
- Do not import another plugin's internals or access its private data. Use host-discovered, structured service/command contracts when those interfaces exist; never create a shortcut that would require bypassing them.
- Treat external plugin execution and AI/tool callers as untrusted. A statically linked Rust plugin is **not sandboxed** merely because it follows a Rust trait or service interface.
- Keep the application usable offline. Do not add mandatory cloud, telemetry, LLM, or external runtime dependencies without authorization.
- Add or update behavior-focused tests. Prefer crate-local `tests/` for public contracts; use private colocated tests only when they serve otherwise inaccessible invariants. Never expose internals solely for tests.
- Keep documentation aligned with implemented changes. Update the **owning** document instead of duplicating policy elsewhere.
- Never fabricate tests, benchmarks, APIs, dependency availability, issue references, runtime behavior, or release readiness.

## Repository and Git hygiene

- Inspect current Git status and branch/worktree before edits. Preserve unrelated staged and unstaged work; never reset or overwrite it to simplify your task.
- Use short-lived task branches and Git worktrees when appropriate; no permanent `develop` branch is required. Do not create, delete, or repoint worktrees without task authorization.
- Follow the [Conventional Commit format](CONTRIBUTING.md#commit-conventions) for commits you are authorized to create.
- Never change Git remotes, rewrite history, or force-push. For maintainer chapter work, validate on the working branch, fast-forward to `main`, run CI, and create the chapter release commit and annotated tag when acceptance and release checks pass. Push the updated `main` branch and tag to the configured remote as part of authorized chapter delivery. External contributors use forks and PRs; crates.io and GitHub Release publication remain separate.
- Keep committed docs and tooling self-contained: no machine-specific paths, references to private parent directories, hidden fixtures, or untracked build dependencies.
- No credentials, personal data, or generated artifacts in commits unless explicitly required and safe to distribute.

## Definition of done and reporting

Before declaring a task complete:

1. Confirm the intended behavior and relevant failure cases are covered by tests or a documented reason they cannot be automated.
2. Run risk-appropriate checks listed in `CONTRIBUTING.md` / `docs/coding-standards.md`. Report the exact commands and outcomes, including limitations.
3. Review the diff for accidental changes, broken paths, security implications, docs drift, and unrelated edits.
4. Update the appropriate documentation when behavior, APIs, architecture, or policy changed.
5. Summarize the implementation, relevant decisions, tests run, and unresolved risks. **Compilation alone is not proof of feature completion.**

Releasing has additional criteria defined only in [docs/releasing.md](docs/releasing.md).

## Document ownership (quick index)

- `README.md` — publicly accurate overview and entry points.
- `CONTRIBUTING.md` — development/PR workflow and Conventional Commits.
- `docs/coding-standards.md` — coding/testing/security engineering requirements.
- `docs/architecture.md` — current architecture and dependency rules.
- `ROADMAP.md` — planned chapters and their acceptance criteria; add a detailed specification only when an active milestone needs one.
- `docs/releasing.md` — release authority, SemVer, Cargo publishing, tags and automation.
- `CHANGELOG.md` — user-visible shipped changes.
- `SECURITY.md` — vulnerability reporting.
