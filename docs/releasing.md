# Versioning and releases

This is tuilane's **authoritative release policy**. See [CONTRIBUTING.md](../CONTRIBUTING.md) for ordinary Conventional Commits and [ROADMAP.md](../ROADMAP.md) for milestones. This policy describes the intended process; **it does not claim that release automation or crates.io publication is already configured**.

## 1. SemVer and milestones

Use [Semantic Versioning 2.0.0](https://semver.org/spec/v2.0.0.html): `MAJOR.MINOR.PATCH`, prefixed by `v` in Git tags.

- **Major:** incompatible changes to established, supported public APIs or user-facing contracts.
- **Minor:** backward-compatible functionality, or a coherent development milestone. During `0.x`, each `0.MINOR.0` may introduce experimental/breaking interfaces, with clear notes.
- **Patch:** a release of suitable compatible fixes or maintenance changes, without a new milestone.
- Pre-releases (`-alpha.N`, `-beta.N`, `-rc.N`) are optional only when distributing an actual prerelease; they are not a substitute for a completed milestone.
- A commit type is **not** an automatic version bump. A `fix` does not require an immediate release; a `feat` may accumulate toward a milestone.
- Roadmap chapter versions are the intended releases for those milestones. A request to implement a chapter through completion authorizes its release-preparation commit and annotated target tag once the chapter acceptance criteria and release checklist pass. If a gate fails, do not release; continue development and revise the target when needed. No invented release dates or artificial commit counts.

Before `v1.0.0`, formally stabilized interfaces must be explicitly identified; all other interfaces may change. Document breaking changes even during `0.x`.

### Future `v1.0.0` stability milestone

`v1.0.0` is an intentional stability and compatibility commitment, not a deadline. It has no predefined release schedule or required development chapter. Completing roadmap chapters, internal tests, or AI-agent validation does not qualify the project automatically. Any number of `0.x` releases, including `v0.13.0`, `v0.14.0`, and later versions, may be appropriate while readiness criteria remain unmet.

Before approving `v1.0.0`, maintainers must determine that:

- Independent users have provided meaningful real-world usage and feedback; internal tests and agent validation cannot substitute for this evidence.
- Testing is sufficient across supported platforms and installation methods.
- Reliability has been demonstrated over multiple pre-1.0 releases.
- Public APIs and plugin contracts intended to be stable are documented and have clear compatibility guarantees.
- Configuration, storage, and migration behavior has been validated.
- Critical bugs and security concerns have been addressed, with remaining limitations understood and documented.
- Long-term maintenance expectations are clear.
- The maintainer explicitly decides the project is ready to make this stability commitment.

## 2. Workspace version authority

Initially, first-party workspace packages share one product version using `[workspace.package].version` and `version.workspace = true` where applicable. This does **not** mean every internal package is independently publishable or must be published.

The source pointed to by a release tag must agree across:

- Root workspace version and all first-party packages participating in the synchronized release.
- `Cargo.lock` and dependency requirements (including registry-compatible versions for published internal crates).
- The executable's `--version` output and applicable package metadata.
- Release notes and changelog headings.
- The Git tag's SemVer version.

**A Git tag does not modify Cargo metadata.** Version changes must already exist in committed source before creating the tag.

Future externally distributed plugins may use independent versions and compatibility ranges, once explicitly designed.

## 3. Release commits and tags

| Event | Dedicated release commit | Annotated version tag |
| --- | --- | --- |
| Ordinary feature or fix | No | No |
| Completed **minor** milestone | **Required** | **Required** |
| Completed **major** milestone | **Required** | **Required** |
| Published **patch** fix | Normally **not** | **Required** |

### Major/minor release commits

Use a focused release-preparation commit after the milestone's implementation and tests are complete:

```text
chore(repo/release): prepare v0.3.0

Finalize the completed Tasks milestone for publication.

- Update workspace and dependency versions and Cargo.lock.
- Curate the changelog and relevant migration notes.
- Align release metadata with the validated source.
```

Only include version/metadata/changelog/release accuracy changes. Do not hide new features, migrations, or unrelated bug fixes in a release commit. Do not create an empty commit merely to mark a version.

### Patch release tags

Prefer a **tag on the final validated fix commit** without a separate `chore(repo/release)` commit. Include the patch version and relevant changelog/metadata updates in that final commit when the release target is known. If metadata must be changed afterward, use a small ordinary metadata commit rather than rewriting published history.

A patch release is a real publication decision, not an automatic consequence of every fix.

### Annotated tags

Use immutable annotated tags of the form `vX.Y.Z` for stable releases:

```sh
git tag -a v0.3.0 -m "tuilane v0.3.0"
```

Tag the exact validated, release-ready commit after it has been integrated on `main`. Do not reuse, rewrite, move, delete, or force-push a published version tag. A Git tag and a GitHub Release are distinct objects.

## 4. Release checklist

Completing an authorized roadmap chapter includes preparing and tagging its target release after all checks below pass. Ordinary feature work, fixes, documentation tasks, or incomplete chapters do not authorize a release. Publishing crates.io packages or creating a GitHub Release remains a separate distribution action.

1. **Scope:** Confirm the intended milestone's acceptance criteria (in `ROADMAP.md` and the active specification) or the patch's fixes. Resolve applicable issues and feedback; an external-contributor PR is not required for maintainer work.
2. **Compatibility:** Review supported APIs/CLI/serialization, migration paths, database backup/recovery, security implications, and known limitations where applicable.
3. **Metadata:** Set intended workspace version, update `Cargo.lock`, registry-compatible workspace dependency version requirements, examples, changelog, and notes.
4. **Validation:** Run full relevant checks from a clean checkout. At minimum, when applicable:

   ```sh
   cargo fmt --all -- --check
   cargo check --workspace --all-targets --locked
   cargo clippy --workspace --all-targets -- -D warnings
   cargo test --workspace --locked
   cargo build --workspace --release --locked
   cargo run -p tuilane -- --version
   ```

   Add actual CLI/TUI smoke tests, restore/migration tests, packaging checks, and supported-target builds as the corresponding features exist.
5. **Tag readiness:** Confirm `git status` is clean, the tag doesn't already exist, all release metadata agrees, and the commit is on/reachable from `main`.
6. **Release commit:** Major/minor requires a dedicated preparation commit; patch normally does not. Validate again when version/metadata changed.
7. **Tag and push:** Create the annotated tag for a completed chapter release, then push `main` and the tag to the configured remote. Never force-push or move a tag. Crates.io or GitHub Release publication is separate and requires configured permissions and explicit authorization.
8. **Distribution:** Only after configured publishing checks pass, publish selected Cargo packages and, where supported, attach verified binary artifacts to a GitHub Release.
9. **Post-release:** Verify the installed package/binaries and their reported versions, link release notes, and record any follow-up issues. Published versions are immutable.

Never describe checks as passing if they were not executed. Chapter delivery may push the validated `main` branch and annotated tag when authorized; crates.io and GitHub Release publication remain separate actions.

## 5. crates.io: binary and workspace dependencies

The intended Cargo installation command is:

```sh
cargo install tuilane --locked
```

This **builds the published binary crate from source**. It does not download a prebuilt binary. There is no claim that the `tuilane` crate or any internal crate names are currently reserved or published.

A publishable application package cannot rely on **path-only, unpublished registry dependencies**. Before the first crates.io release:

1. Check actual availability/ownership of every intended crate name.
2. Choose which supporting crates must be published versus kept truly internal; avoid publishing empty placeholder crates.
3. Ensure publishable internal dependencies specify compatible registry versions **in addition to local workspace paths** where needed, and package metadata/license/readme are correct.
4. Verify the full published dependency graph, including dependency-order publication and release timing. Validate each candidate with `cargo package -p <package>` and/or `cargo publish -p <package> --dry-run` as applicable.
5. Publish supporting registry crates **in dependency order**, wait for registry availability/index propagation as necessary, then publish the `tuilane` binary crate.
6. Confirm a clean external `cargo install tuilane --locked` works after publication.

**Important:** crates.io versions cannot be overwritten or deleted. Partial publication failures must be handled by repair/retry or an appropriate new version; never attempt to republish a different artifact under an existing version. See the [Cargo publishing guide](https://doc.rust-lang.org/cargo/reference/publishing.html).

Early version tags may remain **source/GitHub-only milestones** until meaningful installable packages, publication ownership and the complete dependency graph are ready. Do not label those tags as crates.io releases. This avoids publishing scaffolding only to satisfy a version number.

## 6. GitHub Actions release automation (target design)

**Target:** a pushed, authorized `v*` tag triggers release checks, then conditionally publishes crates.io packages and creates a GitHub Release with prebuilt artifacts for explicitly supported targets. **Do not assume this automation exists until the workflow is inspected and its behavior tested.**

Required automation properties:

- Validate the complete tag as SemVer (not merely matching the `v*` glob), its referenced commit, and the synchronized workspace versions.
- Separate ordinary PR/branch CI from publication permissions.
- Re-run release validation from the tagged source, including `cargo package`/publish checks.
- Require maintainer-controlled approval or a protected publishing environment for irreversible publication; keep credentials scoped to the publishing step. Prefer supported short-lived authentication over long-lived credentials when available.
- Publish only the intended packages, in correct order. Avoid re-publishing existing immutable versions on workflow reruns.
- Build/upload only actually supported and tested target artifacts. Verify checksums and release metadata when implemented.
- On failure, report which steps completed (including any irreversibly published crates) and avoid claiming success.

**Not part of this documentation package:** changing `.github/workflows/ci.yml`, registering crates.io credentials, or activating package publication. Those require inspection of real repository source and maintainer-controlled external configuration.

## 7. Relationship to progress tracking

- [ROADMAP.md](../ROADMAP.md) owns planned versions and milestone exit criteria.
- A detailed milestone specification may supplement `ROADMAP.md` when more scope detail is needed; it must not become a second roadmap.
- GitHub Issues/Milestones track live tasks, defects, PR completion and progress; they are not a second roadmap.
- [CHANGELOG.md](../CHANGELOG.md) records actual user-visible changes, with an `Unreleased` section; it must not treat future roadmap items as shipped.
- The Git tag identifies the exact released source. GitHub Releases and crates.io are separate publication channels and may have different readiness gates.

Do not change the release policy in a random README, agent instruction or issue comment; update this document and link to it.
