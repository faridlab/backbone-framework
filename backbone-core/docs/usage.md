# 🚀 Backbone Core — Usage

**Status:** ✅ Current · **Last Updated:** 2026-06-06

A code-first walkthrough: from an empty crate to a running CRUD API. For the endpoint
contract see [api-reference.md](api-reference.md); for the design behind it see
[architecture.md](architecture.md).

## 1. Add the dependency

```toml
[dependencies]
backbone-core = { path = "../backbone-core" }            # or git tag in downstream projects
# Optional capabilities (all default-off):
# backbone-core = { path = "...", features = ["postgres"] }   # Postgres repositories
# backbone-core = { path = "...", features = ["openapi"] }    # OpenAPI ToSchema derives
```

## 2. Define your entity

An entity implements `PersistentEntity` (id + timestamps + soft-delete). Most fields are
plain data; the trait gives the framework the hooks it needs.

```ignore
use backbone_core::PersistentEntity;
use serde::{Serialize, Deserialize};
use chrono::{DateTime, Utc};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Product {
    pub id: String,
    pub name: String,
    pub price_cents: i64,
    pub created_at: Option<DateTime<Utc>>,
    pub updated_at: Option<DateTime<Utc>>,
    pub deleted_at: Option<DateTime<Utc>>,
}

// impl PersistentEntity for Product { … }  // id/timestamps/soft-delete accessors
```

## 3. Define DTOs

Three roles, all plain serde types:

```ignore
#[derive(Deserialize)]                 // request body for POST
pub struct CreateProduct { pub name: String, pub price_cents: i64 }

#[derive(Deserialize)]                 // request body for PUT/PATCH
pub struct UpdateProduct { pub name: Option<String>, pub price_cents: Option<i64> }

#[derive(Serialize)]                   // response shape (From<Product>)
pub struct ProductResponse { pub id: String, pub name: String, pub price_cents: i64 }
impl From<Product> for ProductResponse { /* … */ }
```

## 4. Get a service

The quickest path is `GenericCrudService` over a repository. Use `InMemoryRepository`
for tests/prototypes, `PostgresRepository` (feature `postgres`) in production.

```ignore
use std::sync::Arc;
use backbone_core::{GenericCrudService, InMemoryRepository};

// Wire the repository → service. FromCreateDto / ApplyUpdateDto map your DTOs to the entity.
let repo = Arc::new(InMemoryRepository::<Product>::new());
let service = Arc::new(
    GenericCrudService::<Product, CreateProduct, UpdateProduct, _>::with_repository(repo),
);
```

To add behaviour, pass a `ServiceLifecycle<Product>` (hooks) and/or a
`CrudEventPublisher<Product>` instead of `with_repository`.

## 5. Mount the router

`BackboneCrudHandler<S, E, C, U, R>::routes` returns an `axum::Router` for a base path.

```ignore
use backbone_core::BackboneCrudHandler;

let app = BackboneCrudHandler::<_, Product, CreateProduct, UpdateProduct, ProductResponse>
    ::routes(service, "/api/v1/products");

let listener = tokio::net::TcpListener::bind("0.0.0.0:8080").await?;
axum::serve(listener, app).await?;
```

That single call mounts the full surface under `/api/v1/products` (read + write). Use
`read_routes` / `write_routes` to split them (e.g. behind different auth).

## 6. Call it

All read endpoints accept pagination, filtering, sorting, search, and sparse fieldsets.
All bodies accept JSON **or** `application/x-www-form-urlencoded`.

### List with pagination, sort, search, and sparse fields

```bash
# page 2, 20 per page, newest first, only id+name+price returned
curl "http://localhost:8080/api/v1/products?page=2&limit=20&sort_by=created_at&sort_order=desc&fields=name,price_cents"

# arbitrary filters are just query keys (status, or any column)
curl "http://localhost:8080/api/v1/products?status=active&search=widget"
```

```jsonc
// 200 OK — PaginatedApiResponse
{
  "success": true,
  "data": [ { "id": "p_1", "name": "Widget", "price_cents": 999 } ],
  "meta": { "total": 42, "page": 2, "limit": 20, "total_pages": 3 }
}
```

> `?fields=` keeps the requested top-level keys **plus `id`**. `fields`, `include`, and
> `with` are reserved — they shape the response and never reach the filter/`WHERE` layer.

### Create / read / update

```bash
curl -X POST http://localhost:8080/api/v1/products \
     -H 'Content-Type: application/json' \
     -d '{ "name": "Widget", "price_cents": 999 }'

curl http://localhost:8080/api/v1/products/p_1

curl -X PATCH http://localhost:8080/api/v1/products/p_1 \
     -H 'Content-Type: application/json' \
     -d '{ "price_cents": 1299 }'
```

### Bulk (atomic) & trash

```bash
# create many
curl -X POST http://localhost:8080/api/v1/products/bulk \
     -d '{ "items": [ {"name":"A","price_cents":1}, {"name":"B","price_cents":2} ] }'

# patch many — shared patch shape
curl -X PATCH http://localhost:8080/api/v1/products/bulk \
     -d '{ "ids": ["p_1","p_2"], "patch": { "price_cents": 0 } }'

# soft-delete many, then restore them
curl -X POST http://localhost:8080/api/v1/products/delete/bulk  -d '{ "ids": ["p_1","p_2"] }'
curl -X POST http://localhost:8080/api/v1/products/restore/bulk -d '{ "ids": ["p_1","p_2"] }'

# list the trash, restore everything
curl "http://localhost:8080/api/v1/products/trash?page=1&limit=20"
curl -X POST http://localhost:8080/api/v1/products/restore/all
```

Bulk operations are all-or-nothing: a missing id (or a duplicate id in an update batch)
rolls back the whole batch with `400` and writes nothing. Batches above
`MAX_BATCH_SIZE` (1,000) are rejected with `400`.

## 7. Feature flags for examples

```bash
# in-memory repo, no DB needed:
cargo run --example basic_usage
# Postgres-backed wiring:
cargo build --features postgres
```

## Going further

- Add validation rules (`EntityValidator`, `RequiredString`, `MaxLength`, …) and domain
  policies (`DomainPolicy`) — see the crate rustdoc.
- Emit/consume `CrudEvent`s via a `CrudEventPublisher`.
- Generate an OpenAPI spec for your concrete entity — see [openapi.md](openapi.md).

→ Endpoint-by-endpoint contract: [api-reference.md](api-reference.md).
