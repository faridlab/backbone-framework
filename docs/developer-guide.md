<!-- Reader: App developer · Mode: Tutorial → How-to -->
# Developer Guide

Get from zero to a Backbone-powered CRUD surface. This page assumes you have a
Rust service (or are starting one) and want Backbone's standard endpoints for
your entities.

> Prerequisites: Rust (edition 2021 toolchain), and Postgres if you use the
> `postgres` backend. Backbone is **not on crates.io** — you depend on it via git
> tags, described next.

## Install

Backbone is distributed as **git dependencies pinned to a release tag**. Add the
crates you need to your service's `Cargo.toml`, pinning every one to the **same
tag** so they stay consistent at one commit:

```toml
[dependencies]
backbone-core = { git = "https://github.com/faridlab/backbone-framework", tag = "v2.6.1", features = ["postgres"] }
backbone-orm  = { git = "https://github.com/faridlab/backbone-framework", tag = "v2.6.1" }
backbone-auth = { git = "https://github.com/faridlab/backbone-framework", tag = "v2.6.1" }
```

> **Never use `branch = "main"`.** It makes every `cargo update` pull HEAD and
> silently drag in breaking changes. Always pin `tag = "v<version>"` and bump the
> tag deliberately. For local work against an unreleased change, temporarily swap
> to `path = "../backbone-framework/backbone-<crate>"` — then revert to a tag pin
> before committing.

Pick the tag from the [release history](../CHANGELOG.md); `v2.6.1` is current at
the time of writing.

## Quickstart

The smallest thing that proves the setup: implement `CrudService` for one entity,
then let `BackboneCrudHandler` generate the whole endpoint set as an Axum router.

```rust
use std::sync::Arc;
use backbone_core::{CrudService, BackboneCrudHandler};

// 1. Implement CrudService for your entity (all standard methods).
//    UserCrudService wraps your repository; User / *Dto are your types.
impl CrudService<User, CreateUserDto, UpdateUserDto> for UserCrudService {
    // list, create, get_by_id, update, partial_update, soft_delete,
    // bulk_create, upsert, list_deleted, restore, empty_trash, … 
}

// 2. Generate all standard endpoints as an Axum Router.
let routes = BackboneCrudHandler::<_, User, CreateUserDto, UpdateUserDto, UserResponse>
    ::routes(Arc::new(user_crud), "/api/v1/users");

// 3. Mount `routes` in your Axum app and serve. Every standard endpoint below
//    now exists for `users`, over HTTP. The same CrudService also backs gRPC.
```

Prototyping without a database? Use the in-memory repository and swap to Postgres
later — the trait is the same:

```rust
use backbone_core::persistence::{InMemoryRepository, PostgresRepository};

let repo = InMemoryRepository::<User>::new();        // tests / prototypes
// let repo = PostgresRepository::<User>::new(pool); // production ("postgres" feature)
```

> These snippets are illustrative wiring taken from `backbone-core`'s own crate
> docs (marked `ignore` there because they need your entity types to compile).
> The **endpoint set, query grammar, and limits below are verified** against the
> crate's [`api-reference.md`](../backbone-core/docs/api-reference.md) and source.
> For a complete, compiling example, see `backbone-core/examples/` and
> [`backbone-core/docs/usage.md`](../backbone-core/docs/usage.md).

## Key concepts

The ideas you need before going further — one line each, follow the link for the
"why":

- **The standard endpoint set** — implementing `CrudService` once yields the same
  CRUD surface for every entity, over HTTP **and** gRPC. See the tables below.
- **Protocol-agnostic core** — your entity code imports no web framework; HTTP and
  gRPC are adapters. ([Philosophy](philosophy.md))
- **Pluggable backends** — Memory/Redis, S3/MinIO/Local, etc., chosen by config,
  not code. ([Architecture](architecture.md))
- **Response envelope** — every response is wrapped in `ApiResponse` /
  `PaginatedApiResponse`; lists carry pagination metadata.
- **Soft-delete + trash lifecycle** — `DELETE` marks deleted; entities live in
  "trash" until restored or emptied.

## The endpoints you get for free

Implementing `CrudService` once and calling `BackboneCrudHandler::routes(...)`
generates **21 HTTP endpoints** per entity. Paths below are relative to the base
you pass (e.g. `/api/v1/products`). This table mirrors the authoritative
catalogue in [`backbone-core/docs/api-reference.md`](../backbone-core/docs/api-reference.md)
— when in doubt, that file is the source of truth.

> **"The 11 Backbone endpoints" is the historical brand** for the classic CRUD +
> trash surface. The generated surface has since grown to 21 (counts, per-id
> trash operations, and the atomic-batch set). You'll still see "11" in older
> crate docs and a stale `STANDARD_ENDPOINT_COUNT = 12` constant — treat
> `api-reference.md` as authoritative for the count. See the
> [glossary](glossary.md#standard-endpoint-set-the-11-backbone-endpoints).

**Read endpoints** (`read_routes`) — 6:

| HTTP | Endpoint | Purpose |
|------|----------|---------|
| `GET` | `{base}` | List active (paginate / filter / sort / `?fields=` / `?include=`) |
| `GET` | `{base}/:id` | Get one active by id |
| `GET` | `{base}/trash` | List soft-deleted |
| `GET` | `{base}/:id/deleted` | Get one soft-deleted by id |
| `GET` | `{base}/count` | Count active |
| `GET` | `{base}/trash/count` | Count soft-deleted |

**Write endpoints** (`write_routes`) — 15:

| HTTP | Endpoint | Purpose |
|------|----------|---------|
| `POST` | `{base}` | Create one |
| `PUT` | `{base}/:id` | Full update one |
| `PATCH` | `{base}/:id` | Partial update one |
| `DELETE` | `{base}/:id` | Soft-delete one |
| `POST` | `{base}/:id/restore` | Restore one |
| `POST` | `{base}/upsert` | Create or update one |
| `POST` | `{base}/bulk` | Create many (`{ "items": [C, …] }`) |
| `PUT` | `{base}/bulk` | Full-update many *(atomic)* |
| `PATCH` | `{base}/bulk` | Partial-update many *(atomic)* — `{ids,patch}` or `{items:[…]}` |
| `POST` | `{base}/delete/bulk` | Soft-delete many *(atomic)* |
| `POST` | `{base}/restore/bulk` | Restore many *(atomic)* |
| `POST` | `{base}/restore/all` | Restore all soft-deleted *(atomic)* |
| `DELETE` | `{base}/trash/bulk` | Permanently delete many trashed *(atomic)* |
| `DELETE` | `{base}/trash/:id` | Permanently delete one trashed |
| `DELETE` | `{base}/empty` | Empty the trash (permanent) |

*Atomic* operations run all-or-nothing in a single transaction, capped at
`MAX_BATCH_SIZE` (1,000); any bad id rolls the whole batch back with `400`. The
same `CrudService` also backs a **gRPC** service exposing these operations —
mount it alongside or instead of HTTP.

## Recipes

### How do I paginate, filter, and sort a list?

```
GET /api/v1/users?page=1&per_page=50&status=active&sort=-created_at
```

`per_page` is clamped to `MAX_PER_PAGE` (100). Filters are entity fields; prefix a
sort key with `-` for descending. An **unknown** filter or sort key returns
`400 Bad Request` (not `500`).

### How do I return only some fields (sparse fieldsets)?

```
GET /api/v1/users?fields=id,email,displayName
```

Projects each object down to the requested top-level keys plus the always-on
`id`. Unknown keys are ignored; omitting `fields` returns everything. `fields`,
`include`, and `with` are reserved keys — they never reach the `WHERE` clause.

### How do I expand a related object (relation expansion)?

```
GET /api/v1/orders?include=customer          (alias: ?with=customer)
```

Hydrates declared to-one relations, injecting each related row as a sibling
object keyed by the relation name. Batched (one `WHERE id = ANY(...)` per
relation — no N+1). Only relations the entity **declares** are honored; unknown
names are ignored.

> **v1 caveat:** the expanded object is the raw related row — it is **not** run
> through the target's response DTO or its `@private` field-security. Do **not**
> enable `?include=` for a target that has private fields. (See
> [CHANGELOG](../CHANGELOG.md).)

### How do I restrict field visibility (`@private` / `@owner`)?

Field-level security is enforced by the entity's repo metadata and an
`AccessScope` (`Platform` | `Tenant(id)`) injected by your auth middleware:
`Platform` sees every field; `Tenant(id)` sees `@private` fields only when the
row's owner equals `id`; an **absent scope fails closed** (private fields
stripped). Security runs **before** sparse projection, so `?fields=` can never
widen the ceiling.

### How do I serve an OpenAPI / Swagger spec?

Enable the default-off feature and merge the shared component document:

```toml
backbone-core = { git = "…", tag = "v2.6.1", features = ["postgres", "openapi"] }
```

Then use `backbone_core::openapi::BackboneComponents`. Not wiring utoipa? Copy
[`backbone-core/docs/openapi.template.yaml`](../backbone-core/docs/openapi.md).

## Configuration

The knobs that matter most (public constants in `backbone-core`):

| Option | Default | When to change |
|--------|---------|----------------|
| `MAX_PER_PAGE` | `100` | Rarely — the ceiling on `per_page`. |
| `MAX_PAGINATION_OFFSET` | `10000` | If clients legitimately page very deep (prefer filters instead). |
| `MAX_BATCH_SIZE` | `1000` | Cap on items per atomic batch request. |
| `openapi` feature | off | Turn on to derive OpenAPI schemas. |
| `postgres` / `database` feature | off | Turn on for the SQLx Postgres backend. |
| backend selection (cache/storage/…) | per crate | Set by config to pick Redis vs Memory, S3 vs Local, etc. |

Runtime configuration is loaded via `backbone-core`'s `config` module (YAML/TOML);
see [`backbone-core/docs/configuration.md`](../backbone-core/docs/configuration.md).

## Troubleshooting

| Symptom | Cause | Fix |
|---------|-------|-----|
| `400 Invalid query parameter or filter: …` | Unknown filter/sort key (often stray camelCase like `sortOrder`) injected as a column name | Use a real entity field; check spelling/case |
| `400 Result set too deep: offset … exceeds the maximum of 10000` | Paged past `MAX_PAGINATION_OFFSET` | Add filters to narrow the result set instead of deep paging |
| `cargo update` pulled a breaking change | Dependency pinned to `branch = "main"` | Pin `tag = "v<version>"` on every backbone crate |
| Crate versions inconsistent at build | Different tags across backbone deps | Pin **all** backbone crates to the same tag |
| A `@private` field leaked via `?include=` | Expanding a relation whose target has private fields (v1 limitation) | Don't enable `?include=` for targets with private fields |
| utoipa not found / OpenAPI types missing | `openapi` feature not enabled | Add `features = ["openapi"]` |
