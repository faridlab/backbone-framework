<!-- Reader: Contributor · Mode: How-to -->
# Contribution Guide

How to propose and land a change in the Backbone workspace. Assumes competence
with Rust and git; gets to the point.

## Dev setup

```bash
git clone https://github.com/faridlab/backbone-framework
cd backbone-framework
cargo build            # build the whole workspace
cargo test             # run all workspace tests
```

Per-crate, while iterating:

```bash
cargo build -p backbone-core
cargo test  -p backbone-orm
cargo build -p backbone-core --features openapi   # prove a feature builds
```

> If you work inside the wider **Metaphor** workspace, prefer the CLI:
> `metaphor build`, `metaphor test`, `metaphor lint check`. From the framework
> repo on its own, the raw `cargo` commands above are the fallback.

## Conventions that gate a PR

1. **Conventional commits.** `type(scope): summary` — e.g.
   `fix(core): reject deep offset with 400`. Types in use here: `feat`, `fix`,
   `docs`, `chore`, `refactor`, `test`. The scope is usually the crate
   (`core`, `orm`, `auth`, …).
2. **No signatures in commit messages.** Do **not** add `Co-Authored-By`,
   "Generated with", or any Claude/AI signature line. This is a hard rule for
   this workspace.
3. **Library-only crates.** No `main.rs` in a framework crate; no `[[bin]]`.
4. **Self-describing manifests.** No `[workspace.dependencies]`, no
   `dep.workspace = true`. Pin direct versions per crate.
5. **Doc comments on every public item** (`///`). New public API without docs is
   incomplete.
6. **Update the CHANGELOG.** Add your change under `## [Unreleased]` in
   [`CHANGELOG.md`](../CHANGELOG.md) (Keep-a-Changelog format), scoped to the crate.

## Where your change goes

- A **bug fix / feature** in an existing crate → that crate's `src/`, in the right
  [architecture layer](architecture.md). Add/adjust tests next to it.
- A **new optional capability** → behind a feature flag, `default` unchanged. See
  the [Maintainer guide](maintainer-guide.md) recipes.
- A **new backend** for an infra crate → behind that crate's trait and an optional
  feature.
- A **new crate** → follow the "add a whole new crate" recipe in the
  [Maintainer guide](maintainer-guide.md), and update the crate tables in both
  [`README.md`](../README.md) and [`architecture.md`](architecture.md).
- An **architectural decision** → write an [ADR](adr/README.md).

## Tests & lint before you push

```bash
cargo test                 # or: cargo test -p <crate> for a focused run
cargo clippy --all-targets # lints
cargo fmt --all            # formatting
```

A PR should build and test **clean** across the workspace. New behavior needs a
test that would fail without your change.

## PR checklist

- [ ] Commits are conventional-commit formatted, **no signature lines**.
- [ ] `cargo build` and `cargo test` pass across the workspace.
- [ ] `cargo clippy` and `cargo fmt --all` are clean.
- [ ] Public items have `///` doc comments.
- [ ] `CHANGELOG.md` updated under `## [Unreleased]`.
- [ ] No `[workspace.dependencies]` / no `main.rs` added to a framework crate.
- [ ] Crate tables updated if you added a crate or a backend.
- [ ] An ADR added if the change makes an architectural decision.

## Review expectations

Reviewers check, in this order: **correctness** (does it do what it claims, with a
test that proves it), **layering** (right crate, right layer, no domain logic in
plumbing), **conventions** (self-describing manifest, feature-gated weight, docs),
and **portability** (nothing that would break lift-and-shift extraction). A change
that violates one of the four non-negotiable rules is sent back regardless of how
good the code is.

## Releasing (maintainers)

Cutting a release is a separate, tag-driven flow — see
[Maintainer guide → Versioning & release](maintainer-guide.md#versioning--release).
Contributors do **not** bump the workspace version in a feature PR; that happens
at release time.
