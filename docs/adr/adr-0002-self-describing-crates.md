# ADR-0002: Self-describing crates; no workspace dependency inheritance

- **Status:** Accepted
- **Date:** 2026-04-24
- **Deciders:** Backbone maintainers

## Context

Cargo offers `[workspace.dependencies]` so member crates can inherit a dependency
version with `dep.workspace = true`, deduplicating version declarations. It is the
conventional choice for a multi-crate workspace. But Backbone's crates were
**extracted byte-for-byte from `monorepo-backbone`**, where each crate declared
its own dependencies, and a founding goal is that any single crate can be
**lifted back out** and used independently. A crate that inherits versions from a
workspace table is not portable on its own — extracting it means reconstructing
the inherited versions.

## Decision

We will keep every member crate's `Cargo.toml` **self-describing**: each crate
declares its own direct dependency versions, and the workspace root
[`Cargo.toml`](../../Cargo.toml) intentionally has **no `[workspace.dependencies]`
table**. The root manifest documents this choice inline so it is not "fixed" by a
well-meaning contributor.

## Alternatives considered

- **Use `[workspace.dependencies]`** — rejected: breaks lift-and-shift
  portability; a crate would no longer compile outside this workspace unchanged.
- **A hybrid (inherit some, pin others)** — rejected: the partial rule is easy to
  violate and hard to review; an all-or-nothing rule is enforceable.

## Consequences

- **Easier:** extracting or independently publishing any crate; reviewing a
  crate's true dependency surface in one file.
- **Harder:** bumping a shared dependency touches multiple manifests; versions can
  drift across crates if not maintained deliberately.
- **We live with:** some duplication of version strings — accepted as the price of
  portability. This is a hard rule: introducing workspace dependency inheritance is
  a reviewable regression.
