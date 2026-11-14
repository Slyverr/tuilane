# Contributing to tuilane

tuilane is personal-first but intended to be maintainable and approachable as an open-source project. Contributions should be **coherent, tested, documented where relevant, and consistent with crate ownership**.

## Before changing code

1. Read [README.md](README.md), the relevant parts of [docs/coding-standards.md](docs/coding-standards.md), and [docs/architecture.md](docs/architecture.md) when changing architecture or public contracts.
2. For planned functionality, check [ROADMAP.md](ROADMAP.md) and the corresponding milestone/issue. A roadmap item is not an implementation guarantee.
3. Inspect the current implementation, tests, open work, and `git status`. Agree on scope with the maintainer for large or incompatible changes.
4. External contributors should keep each PR focused on one coherent behavior or infrastructure improvement. Significant architectural changes should be discussed before implementation and recorded as an ADR when accepted.

## Local development

Use the Rust toolchain selected by `rust-toolchain.toml` with `rustfmt`, `clippy`,
and a native linker. Alternatively, `nix develop` provides the project's pinned
Rust tools and Nix formatter. From the workspace root:

```sh
cargo fmt --all -- --check
cargo check --workspace --all-targets
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cargo run -p tuilane
cargo run -p tuilane -- --help
cargo run -p tuilane -- --version
```

Use `cargo fmt --all` to apply formatting when needed. Select targeted checks during iteration and run full relevant checks before integration. Use `--locked` for reproducible release checks once the lockfile and environment support them; see [releasing](docs/releasing.md). Do not claim a command succeeded unless actually run.

See [docs/coding-standards.md](docs/coding-standards.md) for test placement, error handling, dependency rules, portability, and the Definition of Done.

## Branches, worktrees, and contribution paths

- `main` is the integration/release branch. Prefer short-lived branches (`feat/…`, `fix/…`, `refactor/…`, `docs/…`) for significant work.
- Git worktrees may be used to develop multiple independent branches in parallel; they remain part of **one** Git repository. A permanent `develop` branch is not required.
- Maintainers may develop on a short-lived branch, validate the complete change, and fast-forward it directly to `main`; run CI on the updated branch and resolve failures before release.
- External contributors work from forks and submit PRs to `main`. PRs should describe intent, design implications, observable changes, tests run, documentation impact, and migration/security consequences where relevant.
- Keep tests and accompanying docs with the feature. Do not combine unrelated changes or force-push published tags.
- Preserve Conventional Commit conventions in the resulting commit history. Maintainers integrate completed work by fast-forwarding to `main`; fork contributors use PRs for review and integration.

Use a pull request template for external contributor PRs when one is provided by the hosting platform.

## Commit conventions

Use [Conventional Commits](https://www.conventionalcommits.org/en/v1.0.0/) with a workspace-aware scope:

```text
type(crate-name/subsystem): concise imperative description

A short paragraph describing what changed and why.

- Optional independently meaningful implementation detail.
- Optional test, migration, or compatibility detail.

Optional standard footer.
```

- The **subject is required**, concise (prefer <=72 characters), imperative, lowercase type/scope, without terminal punctuation.
- A meaningful body is recommended for substantial changes; subject-only is acceptable for trivial, self-evident commits.
- Bullets and footers are optional; include them only when informative. Separate body, bullets, and trailers with blank lines.
- Use the actual package name for changes primarily in one crate, e.g. `feat(tuilane-core/services): add provider discovery`. A shorter `fix(tuilane-ui): …` is also valid.
- For repository-wide changes omit the scope, e.g. `build: align workspace metadata`, `docs: clarify milestone acceptance`.
- For a change spanning multiple crates, choose one meaningful shared scope. Do not invent scopes or list every touched crate.
- Prefer one coherent logical change per commit; avoid artificial microcommits or massive unrelated commits.

| Type | Purpose |
| --- | --- |
| `feat` | Add functionality |
| `fix` | Correct a defect |
| `refactor` | Restructure without intended behavior changes |
| `perf` | Improve performance |
| `test` | Add/improve tests |
| `docs` | Documentation-only changes |
| `build` | Manifest, dependencies, build tooling |
| `ci` | Automation/workflow changes |
| `chore` | Other maintenance |
| `style` | Nonfunctional formatting |
| `revert` | Revert a previous commit |

Examples:

```text
feat(tuilane-core/services): support optional providers

Add capability discovery so consumers can detect compatible
providers without importing another plugin's implementation.

- Reject incompatible contract versions.
- Cover missing-provider responses in integration tests.
```

```text
fix(tuilane-ui/focus): restore focus after closing overlays
```

Use `!` and a `BREAKING CHANGE:` footer for changes to supported public contracts that require consumer migration. Do not classify private refactoring as breaking merely because implementation code moved.

Standard trailers include `Refs: #24`, `Fixes: #42`, `Closes: #31`, and genuine `Co-authored-by: Name <email>`. Never invent issue numbers or credits. The release-commit and tag rules are owned by [docs/releasing.md](docs/releasing.md), not this document.

## Change review checklist

- [ ] Requested behavior and important error cases are handled.
- [ ] Public interfaces, layer boundaries, and plugin ownership remain valid.
- [ ] Relevant public contract/integration tests are added or adjusted.
- [ ] Applicable lint, format, build, and test checks ran successfully (or failures are disclosed).
- [ ] Documentation, changelog, migrations, and examples are updated **where needed**.
- [ ] Diff contains no unrelated work, untracked dependencies, secrets, or machine-specific paths.
- [ ] Performance or security-sensitive claims are supported by measurements/tests.

## Reporting issues and vulnerabilities

Use GitHub issues for ordinary bugs and proposals; include reproduction steps, environment, actual and expected behavior, and relevant sanitized logs. Report vulnerabilities privately according to [SECURITY.md](SECURITY.md), **not** in public issues.

## Releases

The [release policy](docs/releasing.md) defines SemVer, milestone readiness, release-preparation commits, annotated tags, and eventual crates.io/GitHub publishing. Completing an authorized roadmap chapter includes its required release commit and tag after acceptance and validation pass. External contributor work does not authorize releases or publication.
