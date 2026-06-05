# 🏛️ Backbone Core — Architecture

**Status:** ✅ Current · **Last Updated:** 2026-06-06

This document explains how `backbone-core` is structured and why. It is written for
contributors and for anyone integrating the crate who wants the mental model rather
than a recipe (for recipes, see [usage.md](usage.md)).

## 🎯 The one idea

Every entity in a Backbone application needs the same things: list, read, create,
update, soft-delete, restore, bulk operations, pagination, filtering. Hand-writing
that per entity is ~250 lines of near-identical code each time. `backbone-core` makes
it **generic**: you supply an entity, its Create/Update DTOs, and a repository — the
crate supplies the service behaviour and the HTTP/gRPC surface.

## 🧱 Layers

```
            ┌─────────────────────────────────────────────┐
  HTTP      │  BackboneCrudHandler<S,E,C,U,R>  (http.rs)   │  axum Router, ~21 routes
  / gRPC    │  GenericGraphQLResolver / gRPC   (grpc.rs)   │
            ├─────────────────────────────────────────────┤
  Service   │  CrudService trait               (http.rs)  │  business-facing contract
            │  GenericCrudService<E,C,U,R>     (service.rs)│  hooks + events + batch
            │  UseCase / CQRS / Policy / Validation        │
            ├─────────────────────────────────────────────┤
  Persist.  │  CrudRepository<E> trait    (persistence/)  │  storage-agnostic
            │  InMemoryRepository | PostgresRepository     │
            ├─────────────────────────────────────────────┤
  Domain    │  PersistentEntity, AggregateRoot,           │  DDD building blocks
            │  ValueObject, Specification                  │
            └─────────────────────────────────────────────┘
```

Each layer depends only on the one below it through a **trait**, never a concrete type.
That is what keeps the HTTP layer reusable across entities and the service layer
reusable across storage backends.

### Module map (`src/`)

| Area | Modules |
|------|---------|
| HTTP / transport | `http.rs`, `extractors.rs`, `grpc.rs`, `graphql.rs` |
| Service / use cases | `service.rs`, `usecase.rs`, `cqrs.rs`, `command.rs`, `query.rs`, `bulk.rs` |
| Persistence | `persistence/` (`traits.rs`, `memory.rs`, `postgres.rs`, `adapter.rs`), `repository.rs`, `macros.rs` |
| Domain (DDD) | `entity.rs`, `aggregate.rs`, `value_object.rs`, `specification.rs`, `domain_service.rs`, `policy.rs`, `validation.rs` |
| Lifecycle / orchestration | `trigger.rs`, `flow.rs`, `state_machine.rs`, `projection.rs`, `integration.rs` |
| Platform | `config/`, `module.rs`, `module_registry.rs`, `registry.rs`, `builder.rs`, `error.rs`, `utils.rs` |
| OpenAPI (feature) | `openapi.rs` |

## 🔌 The generic CRUD handler

`BackboneCrudHandler<S, E, C, U, R>` is the heart of the HTTP layer. Its five type
parameters are the contract you fill in per entity:

| Param | Meaning | Bound |
|-------|---------|-------|
| `S` | Your service | `CrudService<E, C, U>` |
| `E` | The entity | `Serialize` |
| `C` | Create DTO | `DeserializeOwned` |
| `U` | Update DTO | `DeserializeOwned` |
| `R` | Response DTO | `From<E> + Serialize` |

It builds an `axum::Router` from a `base_path` (e.g. `/api/v1/products`). Helpers:

- `routes(service, base_path)` — all read + write routes.
- `read_routes(service, base_path)` — GET-only (list, get, trash, counts).
- `write_routes(service, base_path)` — mutating routes (create, update, delete, bulk).

**Why handlers are generic (and why OpenAPI is hand-assisted).** Because the handlers
are generic over `E/C/U/R` and `base_path` is a runtime string, the crate cannot stamp
out a concrete per-entity OpenAPI spec — utoipa's path macro needs concrete types and
literal paths. The crate therefore derives `ToSchema` on its shared concrete types and
ships a reusable component template; downstream crates assemble the per-entity spec.
See [openapi.md](openapi.md).

### Route precedence

Routes are registered so that static segments win over `:id` captures — e.g.
`/{base}/trash/bulk` and `/{base}/restore/all` are matched before `/{base}/:id`. This
is why the bulk and trash endpoints can coexist with id-based ones.

## 🔁 Request lifecycle

```
  HTTP request
      │
      ▼
  JsonOrForm extractor ── decodes JSON *or* x-www-form-urlencoded (lenient: JSON fallback)
      │
      ▼
  Handler ── parses ListQueryParams, strips reserved keys (fields/include/with),
      │       validates pagination depth + batch size (→ 400 on violation)
      ▼
  CrudService ── before_* hook → repository call → after_* hook → publish CrudEvent
      │
      ▼
  Repository ── storage (single transaction for atomic batch ops)
      │
      ▼
  Response ── ApiResponse / PaginatedApiResponse, optionally sparse-projected (?fields=)
```

### Lifecycle hooks & events

`GenericCrudService` takes a `ServiceLifecycle<E>` (default `NoOpLifecycle`) with
`before_create/after_create/before_update/after_update/before_delete/after_delete`, and
a `CrudEventPublisher<E>` that receives `CrudEvent::{Created, Updated, SoftDeleted,
Restored, HardDeleted}` per affected entity — batch operations fire one event per row,
exactly like the single-row methods.

## 🗑️ Soft-delete & trash model

Entities carry `deleted_at`. Soft-delete sets it; the entity stays in storage but is
excluded from normal `list`/`get`. The `trash` endpoints query the deleted set
explicitly, `restore` clears `deleted_at`, and `permanent_delete` / `empty_trash`
remove rows for good. Batch variants (`restore/bulk`, `delete/bulk`, `trash/bulk`,
`restore/all`) run in a single transaction with all-or-nothing semantics.

## 📐 Guard rails (constants)

| Constant | Value | Purpose |
|----------|-------|---------|
| `MAX_PER_PAGE` | 100 | Page size is clamped here before computing the offset |
| `MAX_PAGINATION_OFFSET` | 10_000 | Requests paging deeper are rejected (`400`) instead of running a slow deep `OFFSET` scan |
| `MAX_BATCH_SIZE` | 1_000 | Batch requests larger than this are rejected (`400`) at both the HTTP and service layer |

### 400 vs 500 classification

A bad filter/sort key (a typo, or a stray `sortOrder`) reaches Postgres and produces
`column "..." does not exist` (SQLSTATE `42703`) or `invalid input syntax`. The handler
classifies these as **client errors → `400`**, not server errors → `500`, so clients get
an actionable signal. Genuine database/server failures still surface as `500`.

## 🧪 Testing seam

`InMemoryRepository` implements the same `CrudRepository<E>` trait as
`PostgresRepository`, so services can be unit-tested with zero database. The
`impl_crud_repository!` macro generates a `CrudRepository` impl for a generated repo
struct, keeping hand-written and generated code in sync.

→ Next: [usage.md](usage.md) to put this together in code.
