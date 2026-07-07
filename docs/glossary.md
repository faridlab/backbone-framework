<!-- Reader: All · Mode: Reference -->
# Glossary — Ubiquitous Language

One term, one definition, used consistently throughout this handbook. If you
introduce a new concept in the docs, define it here first.

### AccessScope
The caller's visibility level for field-level security, read from an Axum
`Extension` and injected by the application's auth middleware. Two variants:
`Platform` (sees every field) and `Tenant(id)` (sees `@private` fields only when
the row's `@owner` equals `id`). An **absent** scope fails closed — private fields
are stripped.

### Atomic batch endpoint
A bulk operation that runs inside a **single transaction** with all-or-nothing
semantics: if any id is missing or already in the target state, the whole batch is
rolled back with `400` and no rows are written. Capped at `MAX_BATCH_SIZE`.

### Backbone Framework
This workspace — a Cargo workspace of 17 focused Rust crates providing the CRUD
core and infrastructure adapters that services and modules build on. The
**plumbing tier** of the wider Metaphor world.

### `BackboneCrudHandler`
The generic Axum router builder in `backbone-core`. Given a `CrudService`
implementation and a base path, it generates the full standard endpoint set as an
Axum `Router` — no per-entity handler code.

### `CrudService`
The core async trait an entity implements once to get the standard endpoint set,
served identically over HTTP and gRPC. The seam between your entity and the
generic CRUD engine.

### Crate (project type)
A focused Rust library — one crate, one concern, `[lib]` only (no `main.rs`),
independently publishable. Every member of this workspace is a crate. Contrast
with `module`, `backend-service`, `cli-tool`.

### Entity
A domain object that gets the standard CRUD surface. You implement `CrudService`
for it; Backbone supplies the endpoints, pagination, soft-delete, and bulk
operations.

### Feature (Cargo feature flag)
The mechanism by which optional weight (a database driver, protobuf types,
OpenAPI, a specific backend) is opted into. The framework keeps `default = []`
where the weight is optional — you pay only for what you enable.

### Field-level security
Per-field visibility enforced on read endpoints via `@private` (fields visible
only to the row owner or `Platform`) and `@owner` (the field naming the owner /
tenant id), evaluated against the caller's `AccessScope`. Runs **before** sparse
projection.

### Git-tag distribution
Backbone's distribution model: consumed as git dependencies pinned to a release
tag (`tag = "v<version>"`), **not** published to crates.io. See
[ADR-0001](adr/adr-0001-git-tag-distribution.md).

### Lift-and-shift discipline
The rule that every crate is self-describing (deps declared per-crate, no
workspace inheritance) so any crate can be extracted and used on its own. The
workspace's founding property. See [ADR-0002](adr/adr-0002-self-describing-crates.md).

### `MAX_BATCH_SIZE`
Public constant (`1000`) — the maximum items in one atomic batch request; larger
payloads are rejected with `400` at both the HTTP and service layers.

### `MAX_PAGINATION_OFFSET`
Public constant (`10000`, ~100 pages at max size) — the deepest offset a list
request may reach before it is rejected with `400` instead of running an expensive
deep `OFFSET` scan.

### `MAX_PER_PAGE`
Public constant (`100`) — the ceiling `per_page` is clamped to on list endpoints.

### Metaphor
The meta-workspace that contains this framework alongside domain `module`
projects, a runnable `backend-service`, and the `metaphor` CLI. Orchestrated by
`metaphor.yaml`. Backbone is one project (`type: crate`) within it.

### Module (project type)
A domain-logic project in the Metaphor workspace (accounting, catalog, …) where
the **schema YAML is the source of truth** and code is generated with regen-safe
`// <<< CUSTOM` markers. Modules build **on** Backbone; the CUSTOM/regen machinery
does **not** apply to the framework crates themselves.

### Monorepo versioning
One version for the whole workspace at a given commit, authoritative in
`[workspace.metadata.release].version`. A major bump means something in the
workspace broke, not necessarily the crate you use. See
[ADR-0004](adr/adr-0004-monorepo-versioning.md).

### `monorepo-backbone`
The internal monorepo Backbone was extracted from (byte-for-byte at `v2.0.0`). The
reason lift-and-shift discipline is treated as sacred.

### Pluggable backend
An interchangeable implementation of an infrastructure crate's trait (e.g.
Redis/Memory for cache, S3/MinIO/Local for storage), selected by **config, not a
code change**.

### Protocol-agnostic core
The principle that domain primitives in `backbone-core` know nothing about HTTP,
gRPC, or GraphQL; transports are adapters. See
[ADR-0003](adr/adr-0003-protocol-agnostic-core.md).

### Relation expansion (`?include=` / `?with=`)
Hydration of declared to-one relations on read endpoints, injecting each related
row as a sibling object keyed by the relation name. Batched (no N+1). Only
declared relations are honored. *v1:* the expanded object is the raw related row —
not run through the target's response DTO or field-security.

### Response envelope
The consistent wrapper around every response — `ApiResponse` for single objects,
`PaginatedApiResponse` for lists (which carry pagination metadata), `BulkResponse`
for batch results.

### Soft delete / trash
The lifecycle where `DELETE` marks an entity deleted rather than removing it;
deleted entities live in "trash" (`/trash`) until `restore`d or permanently
removed (`empty`).

### Sparse fieldset (`?fields=`)
Projection of a read response down to the requested top-level keys plus the
always-on `id`. Runs **after** field-level security, so it can never widen the
visibility ceiling. `fields`, `include`, `with` are reserved keys, stripped before
the filter map reaches the repository.

### Standard endpoint set ("the 11 Backbone endpoints")
The consistent CRUD surface every entity gets from one `CrudService`
implementation: **21 HTTP endpoints** — 6 read (`read_routes`: list, get,
list-trash, get-trashed, count, trash-count) and 15 write (`write_routes`:
create, update, partial-update, soft-delete, restore, upsert, bulk-create, the
six atomic-batch operations, permanent-delete-one, and empty-trash). The same
service also backs an equivalent gRPC surface. The authoritative catalogue is
[`backbone-core/docs/api-reference.md`](../backbone-core/docs/api-reference.md).

> **Naming note (reconciled):** "the 11 Backbone endpoints" is the **historical
> brand** for the original CRUD + trash surface. The generated surface has since
> grown to **21**. The source still carries stale counts — a
> `STANDARD_ENDPOINT_COUNT = 12` constant in `lib.rs` and "11"/"16 endpoints"
> doc-comments in `http.rs`. These are known drift; `api-reference.md` is the
> authoritative count. (Changing the public `STANDARD_ENDPOINT_COUNT` constant is
> a maintainer decision — a docs pass leaves the code untouched.)
