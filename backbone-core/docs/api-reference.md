# 📡 Backbone Core — API Reference

**Status:** ✅ Current · **Last Updated:** 2026-06-06

The HTTP contract produced by `BackboneCrudHandler::routes(service, base_path)`. Paths
below are relative to `{base}` (e.g. `/api/v1/products`). All request bodies accept
`application/json` or `application/x-www-form-urlencoded` (JSON is the lenient default).

## 🧭 Endpoint catalogue

### Read endpoints (`read_routes`)

| Method | Path | Purpose | Success | Response |
|--------|------|---------|---------|----------|
| GET | `{base}` | List active (paginate / filter / sort / search / `?fields=`) | 200 | `PaginatedApiResponse<R>` |
| GET | `{base}/:id` | Get one active by id (`?fields=`) | 200 | `ApiResponse<R>` |
| GET | `{base}/trash` | List soft-deleted (paginate / filter / `?fields=`) | 200 | `PaginatedApiResponse<R>` |
| GET | `{base}/:id/deleted` | Get one soft-deleted by id (`?fields=`) | 200 | `ApiResponse<R>` |
| GET | `{base}/count` | Count active entities | 200 | `ApiResponse<u64>` |
| GET | `{base}/trash/count` | Count soft-deleted entities | 200 | `ApiResponse<u64>` |

### Write endpoints (`write_routes`)

| Method | Path | Purpose | Success | Request body |
|--------|------|---------|---------|--------------|
| POST | `{base}` | Create one | 201 | `C` (create DTO) |
| PUT | `{base}/:id` | Full update one | 200 | `U` (update DTO) |
| PATCH | `{base}/:id` | Partial update one | 200 | object of fields |
| DELETE | `{base}/:id` | Soft-delete one | 200 | — |
| POST | `{base}/:id/restore` | Restore one | 200 | — |
| POST | `{base}/upsert` | Create or update one | 201 | `C` (create DTO) |
| POST | `{base}/bulk` | Create many | 201 | `{ "items": [C, …] }` |
| PUT | `{base}/bulk` | Full-update many (atomic) | 200 | `[{ "id", …U }]` |
| PATCH | `{base}/bulk` | Partial-update many (atomic) | 200 | shared or per-id (below) |
| POST | `{base}/delete/bulk` | Soft-delete many (atomic) | 200 | `{ "ids": [...] }` |
| POST | `{base}/restore/bulk` | Restore many (atomic) | 200 | `{ "ids": [...] }` |
| POST | `{base}/restore/all` | Restore all soft-deleted (atomic) | 200 | — |
| DELETE | `{base}/trash/bulk` | Permanently delete many trashed (atomic) | 200 | `{ "ids": [...] }` |
| DELETE | `{base}/trash/:id` | Permanently delete one trashed | 204 | — |
| DELETE | `{base}/empty` | Empty the trash (permanent) | 200 | — |

> **Route precedence:** static segments are registered before `:id` captures, so
> `/trash/bulk`, `/restore/all`, `/delete/bulk` never collide with `/:id`.

## 🔎 Query parameters (`ListQueryParams`)

Applies to `GET {base}`, `GET {base}/trash`, and the `?fields=` projection on the
single-get endpoints.

| Param | Type | Default | Notes |
|-------|------|---------|-------|
| `page` | `u32` | 1 | 1-indexed |
| `limit` | `u32` | 20 | Clamped to `MAX_PER_PAGE` = 100 |
| `sort_by` | `string` | — | Field/column name |
| `sort_order` | `string` | — | `asc` / `desc` |
| `search` | `string` | — | Free-text search term |
| `status` | `string` | — | Common status filter |
| `fields` | `string` | — | Sparse fieldset (reserved, see below) |
| *(any other key)* | `string` | — | Becomes a filter passed to the repository |

### Sparse fieldsets (`?fields=`)

`?fields=a,b,c` trims each response object to the requested top-level keys **plus the
always-on `id`**. Comma-separated, whitespace-trimmed; unknown keys are ignored; an
absent/empty value returns every field. `fields`, `include`, and `with` are **reserved**
response-shaping keys — stripped before filters reach the repository, so they never leak
into the `WHERE` clause.

### Field-level security (`@private` / `@owner`)

Read endpoints strip an entity's `@private` fields from the serialized response
unless the caller is allowed to see them. Visibility is decided by an
`AccessScope` (`backbone_core::AccessScope`) that the application's auth
middleware injects as an axum `Extension`:

| Scope | Sees `@private` fields |
|-------|------------------------|
| `Platform` | Always (superadmin / root) |
| `Tenant(id)` | Only when the row's `@owner` field equals `id` |
| *(no extension)* | Never — **fails closed** |

An entity opts in by overriding two `backbone_orm::EntityRepoMeta` hooks (both
default to no-op, so existing entities are unaffected):

```rust
fn private_fields() -> &'static [&'static str] { &["hppPerUnit"] } // response keys (camelCase)
fn owner_field() -> Option<&'static str> { Some("providerId") }    // response key holding the owner id
```

Security runs **before** sparse projection, so the visibility ceiling always
beats a `?fields=` request — a client cannot recover a stripped `@private` field
by naming it in `?fields=`. Names are matched against the **response JSON keys**
(camelCase), not DB columns. A `Tenant` scope against a row whose `@owner` is
`null` is treated as non-owner (only `Platform` sees private fields).

### Pagination depth

The effective offset is `max(page-1, 0) * clamp(limit, 1, 100)`. If it exceeds
`MAX_PAGINATION_OFFSET` (10,000), the request is rejected with `400` rather than running
an expensive deep `OFFSET` scan:

```
Result set too deep: offset 10100 exceeds the maximum of 10000. Please add filters to narrow your search.
```

## 📦 Request body shapes

### Bulk create — `POST {base}/bulk`
```json
{ "items": [ { "name": "A" }, { "name": "B" } ] }
```

### Bulk full update — `PUT {base}/bulk`
Array of objects, each an `id` plus the flattened update DTO:
```json
[ { "id": "p_1", "name": "A", "price_cents": 1 },
  { "id": "p_2", "name": "B", "price_cents": 2 } ]
```

### Bulk partial update — `PATCH {base}/bulk` (two auto-detected shapes)
```jsonc
// Shared: one patch applied to many ids
{ "ids": ["p_1", "p_2"], "patch": { "price_cents": 0 } }

// Per-item: a distinct patch per id
{ "items": [ { "id": "p_1", "patch": { "price_cents": 0 } },
             { "id": "p_2", "patch": { "name": "B2" } } ] }
```

### Id-list bodies — `BatchIdsRequest`
Used by `delete/bulk`, `restore/bulk`, `trash/bulk`:
```json
{ "ids": ["p_1", "p_2", "p_3"] }
```

Batches larger than `MAX_BATCH_SIZE` (1,000) → `400`. Update/patch batches that repeat an
id, or id-list operations referencing a missing id, roll back the whole batch → `400`.

## 📨 Response envelopes

### `ApiResponse<T>` — single resource
```jsonc
{ "success": true, "data": { /* T */ }, "message": "…optional" }
// error:
{ "success": false, "error": "…" }
```

### `PaginatedApiResponse<T>` — list endpoints
```jsonc
{
  "success": true,
  "data": [ /* T, … */ ],
  "meta": { "total": 150, "page": 1, "limit": 20, "total_pages": 8 }
}
// error: same shape with "success": false, "data": [], and "error": "…"
```

### `BulkResponse<T>`
```json
{ "items": [ /* T, … */ ], "total": 2, "failed": 0, "errors": [] }
```

## 🚦 Status codes

| Code | When |
|------|------|
| 200 OK | List, get, update, soft-delete, restore, counts, empty-trash, bulk (200) |
| 201 Created | `POST {base}`, `POST {base}/bulk`, `POST {base}/upsert` |
| 204 No Content | `DELETE {base}/trash/:id` (permanent delete, no body) |
| 400 Bad Request | Bad filter/sort key (`42703` / invalid syntax), pagination too deep, batch too large, duplicate/missing id in a batch, malformed body |
| 404 Not Found | Entity (or trashed entity) not found by id |
| 409 Conflict | Create conflicts with an existing unique entity |
| 500 Internal Server Error | Genuine database/server failure |

→ For OpenAPI/Swagger generation of this surface for your concrete entity, see
[openapi.md](openapi.md).
