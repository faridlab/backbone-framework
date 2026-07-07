<!-- Reader: Maintainer · Mode: How-to -->
# Maintainer Guide

How to maintain the Backbone workspace and extend it without breaking the
conventions that make it work. This is a **`crate` workspace**, not a `module`
project — so there is **no schema-YAML SSoT and no `// <<< CUSTOM` regeneration**
here. Code is hand-written and hand-reviewed. (Those mechanisms live in the
Metaphor `module` projects, outside this workspace.)

## Before you touch anything

- Read the workspace [`Cargo.toml`](../Cargo.toml) and this project's
  [`CLAUDE.md`](../CLAUDE.md).
- Remember the project type: **`crate`**. The rules that follow flow from it —
  library only (no `main.rs`), self-describing manifests, plumbing not domain.
- Skim the [Architecture](architecture.md) so you put new code in the right layer.

## The four rules that are never negotiable

1. **Libraries only.** Every member crate is `[lib]`, no `[[bin]]`. If you need a
   binary, it belongs in a `cli-tool` or `backend-service` project, not here.
2. **Self-describing manifests.** Do **not** add `[workspace.dependencies]` or
   inherit deps with `x.workspace = true`. Pin direct versions in each crate's
   own `Cargo.toml`. This preserves lift-and-shift extractability
   ([ADR-0002](adr/adr-0002-self-describing-crates.md)).
3. **No domain logic.** Crates are plumbing. Business rules go in `module`
   projects. A crate growing domain nouns is a layering bug.
4. **No heavy default features.** `default = []` where the weight is optional.
   Gate databases, protobuf types, OpenAPI, and backends behind features.

## Where code goes (within `backbone-core`)

| Layer | Holds | May depend on |
|-------|-------|---------------|
| Domain | `entity`, `aggregate`, `value_object`, `state_machine`, `flow`, `policy`, `cqrs` | nothing |
| Application | `service`/`crud`, `usecase`, `registry`, `module` | domain |
| Infrastructure | `persistence`, `repository`, `config`; `backbone-orm` | domain, application |
| Presentation | `http`, `grpc`, `graphql`, `extractors`, `openapi` | application |

Adapters (cache, storage, email, …) each live in their **own crate**, never
inside `backbone-core`.

## Recipe: add an optional capability to an existing crate

Follow the pattern the `openapi` feature already set:

1. Add the dependency **optional**: `utoipa = { version = "5", optional = true }`.
2. Declare a feature that enables it: `openapi = ["dep:utoipa"]` in `[features]`.
   Keep `default = []` so the base build is unchanged.
3. Guard the new code with `#[cfg(feature = "openapi")]`.
4. Document it: a doc comment on the public items, plus a line in the crate's
   `docs/` (for `backbone-core`) and the [CHANGELOG](../CHANGELOG.md).
5. Build **both** ways to prove the default graph is clean:
   ```bash
   cargo build -p backbone-core                 # default: feature absent
   cargo build -p backbone-core --features openapi
   ```

## Recipe: add a new backend to an infrastructure crate

Backends sit behind the crate's trait (the "pluggable backends" principle):

1. Implement the crate's storage/transport trait for the new backend in its own
   module (e.g. `backbone-storage/src/gcs.rs`).
2. Put its heavy client dependency behind an optional feature (`gcs = ["dep:…"]`).
3. Wire it into the crate's config/factory so selection is a **config value, not a
   code change** for consumers.
4. Add it to the backend list in that crate's `README.md` and this handbook's
   [Architecture](architecture.md) table.
5. Test against the new backend; keep the in-memory/local backend as the default
   for the crate's own test suite.

## Recipe: add a whole new crate to the workspace

1. Create `backbone-<concern>/` with a self-describing `Cargo.toml` (`[lib]`, no
   `[[bin]]`, pinned direct deps, `default = []`).
2. Add it to `members` in the workspace [`Cargo.toml`](../Cargo.toml) — **and to
   the crate tables** in [`README.md`](../README.md) and
   [Architecture](architecture.md) (don't repeat the current README drift).
3. Give it a `README.md` (what it does in five lines) and doc comments on every
   public item.
4. If it introduces a new architectural decision, write an
   [ADR](adr/README.md).

## Versioning & release

The workspace uses **monorepo versioning**
([ADR-0004](adr/adr-0004-monorepo-versioning.md)): **one version covers every
member crate at a commit**, and it is authoritative in **one place** —
`[workspace.metadata.release].version` in [`Cargo.toml`](../Cargo.toml)
(currently `2.6.1`).

> Note: individual crates still carry their own `version` field (many read
> `2.0.0`, frozen from the byte-for-byte extraction). These are **not** the
> release source of truth — the `[workspace.metadata.release]` field is. Do not
> rely on per-crate versions to reason about a release.

Semver applies to the workspace **as a whole**:

- **Patch** (`2.6.1 → 2.6.2`) — bug fixes; safe to adopt.
- **Minor** (`2.6.1 → 2.7.0`) — additive; safe to adopt.
- **Major** (`2.6.1 → 3.0.0`) — a breaking change *somewhere*; consumers opt in.

To cut a release:

1. Bump `[workspace.metadata.release].version` in `Cargo.toml`.
2. Move the `## [Unreleased]` notes into a `## [x.y.z]` section in
   [`CHANGELOG.md`](../CHANGELOG.md) (Keep-a-Changelog format).
3. Build and test the whole workspace clean.
4. Push a `v<version>` git tag. The release workflow
   (`.github/workflows/release.yml`) **refuses to publish** a tag whose version
   does not match the `Cargo.toml` field, then builds/tests and publishes a
   GitHub Release using the matching CHANGELOG section as the body.

## What will break things

- Adding a `main.rs` to any framework crate (wrong project type).
- Introducing `[workspace.dependencies]` or `workspace = true` deps (kills
  extractability).
- Leaking domain logic into a crate.
- A heavy **default** feature that forces consumers to pull deps they don't use.
- Publishing a `v*` tag whose version doesn't match `Cargo.toml` (the workflow
  rejects it — this is a guard, not a bug).
- Re-exporting an entire dependency as your own public API (creates tight
  coupling).
