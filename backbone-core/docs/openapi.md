# 🧾 Backbone Core — OpenAPI / Swagger Guide

**Status:** ✅ Current · **Last Updated:** 2026-06-06 · **Feature:** `openapi`

How to produce and serve an OpenAPI (Swagger) document for a Backbone CRUD service.

## Why core doesn't emit a finished per-entity spec

`backbone-core`'s handlers are generic — `BackboneCrudHandler<S, E, C, U, R>` — and the
mount path (`base_path`) is a **runtime** string. utoipa's `#[utoipa::path(...)]` macro,
which generates path items, needs **concrete** types and **literal** paths at compile
time. So core cannot, by itself, stamp out a complete spec for `/api/v1/products` with a
`Product` schema.

Instead, with `--features openapi`, core gives you the reusable pieces and you assemble
the per-entity spec in your service crate. Two supported approaches:

| | Path A — utoipa (recommended) | Path B — YAML template |
|---|---|---|
| Compile-time deps | `utoipa` (+ a UI crate to serve) | none |
| Stays in sync with code | ✅ generated from your types | ⚠️ hand-maintained |
| Best for | Rust services that want live, typed docs | polyglot teams / quick publishing |

---

## Path A — generate with utoipa

### 1. Enable the feature

```toml
# your service crate
backbone-core = { path = "../backbone-core", features = ["openapi"] }
utoipa = { version = "5", features = ["uuid", "chrono"] }
```

> Keep your `utoipa` **major** version aligned with backbone-core's (currently `5.x`),
> so the shared `ToSchema` impls and your derives agree.

### 2. What core provides

- `utoipa::ToSchema` on the shared envelope/request types: `ApiResponse<T>`,
  `PaginatedApiResponse<T>`, `PaginationResponse`, `BulkResponse<T>`,
  `BulkCreateRequest<T>`, `UpsertRequest<T>`, `BatchIdsRequest`, `BulkUpdateItem<U>`,
  `BulkPatchItem`, `BulkPatchRequest`, `FilterOptions`, `SortOrder`, `ListQueryParams`.
- `backbone_core::BackboneComponents` — a `utoipa::OpenApi` document that registers the
  **non-generic** component schemas, ready to merge into your aggregator.

### 3. Annotate your entity and paths

Derive `ToSchema` on your concrete DTOs, then write thin `#[utoipa::path]` items that
describe the routes you mounted (path literals must match your `base_path`).

```ignore
use utoipa::{OpenApi, ToSchema};
use backbone_core::{ApiResponse, PaginatedApiResponse, BackboneComponents};

#[derive(ToSchema, serde::Serialize)]
struct ProductResponse { id: String, name: String, price_cents: i64 }

#[derive(ToSchema, serde::Deserialize)]
struct CreateProduct { name: String, price_cents: i64 }

/// List products.
#[utoipa::path(
    get, path = "/api/v1/products",
    params(
        ("page" = Option<u32>, Query, description = "1-indexed page"),
        ("limit" = Option<u32>, Query, description = "Page size (max 100)"),
        ("fields" = Option<String>, Query, description = "Sparse fieldset, e.g. id,name"),
    ),
    responses((status = 200, body = PaginatedApiResponse<ProductResponse>)),
)]
async fn list_products() {}

/// Create a product.
#[utoipa::path(
    post, path = "/api/v1/products",
    request_body = CreateProduct,
    responses(
        (status = 201, body = ApiResponse<ProductResponse>),
        (status = 409, description = "Conflicts with an existing product"),
    ),
)]
async fn create_product() {}
```

### 4. Aggregate into one document

```ignore
#[derive(OpenApi)]
#[openapi(
    paths(list_products, create_product /*, … */),
    components(schemas(
        ProductResponse,
        CreateProduct,
        ApiResponse<ProductResponse>,
        PaginatedApiResponse<ProductResponse>,
    )),
    // pull in the shared, non-generic component schemas from core:
    nest((path = "/", api = BackboneComponents)),
)]
struct ApiDoc;
```

`ApiDoc::openapi()` now yields a `utoipa::openapi::OpenApi` you can serialize or serve.

---

## Path B — adapt the YAML template

If you don't want compile-time deps, copy [`openapi.template.yaml`](openapi.template.yaml),
replace the placeholders, and commit the result per service:

| Placeholder | Replace with | Example |
|-------------|--------------|---------|
| `{collection}` | your collection path segment | `products` |
| `{basePath}` | full mount base | `/api/v1/products` |
| `{ItemSchema}` | your entity response schema name | `Product` |
| `{CreateSchema}` / `{UpdateSchema}` | your DTO schema names | `CreateProduct` |

The template already encodes the 21 endpoints, the `ListQueryParams` query grammar,
the response envelopes, and the error responses — you only fill in the entity-specific
schemas.

---

## Serving the spec

`backbone-core` deliberately **does not bundle a UI** (no `utoipa-swagger-ui` /
`utoipa-redoc` dependency) — a foundation crate shouldn't force a UI asset bundle on
every consumer. Pick one in your service crate:

### Swagger UI

```toml
utoipa-swagger-ui = { version = "8", features = ["axum"] }
```
```ignore
use utoipa_swagger_ui::SwaggerUi;
let app = app.merge(
    SwaggerUi::new("/swagger-ui").url("/api-docs/openapi.json", ApiDoc::openapi()),
);
// → browse http://localhost:8080/swagger-ui
```

### Redoc / Scalar

Add `utoipa-redoc` or `utoipa-scalar` (same pattern), or just expose the raw document
and point any viewer at it.

### No UI — raw document route

```ignore
async fn openapi_json() -> axum::Json<utoipa::openapi::OpenApi> {
    axum::Json(ApiDoc::openapi())
}
let app = app.route("/api-docs/openapi.json", axum::routing::get(openapi_json));
```

For Path B, serve the static YAML/JSON file directly.

---

## Caveats

| Thing | Behaviour in the generated schema |
|-------|-----------------------------------|
| `ListQueryParams.filters` (`#[serde(flatten)]` map) | Arbitrary filter keys are an open map — not individually enumerable; document the well-known ones (`status`, `search`, …) as explicit params. |
| `BulkUpdateItem<U>.data` (flattened generic DTO) | Rendered as an open object in core; in your spec, reference the concrete `Update<Entity>` schema directly. |
| `BulkPatchRequest` (`#[serde(untagged)]`) | Renders as `oneOf` (shared vs per-item shape). |
| Generic envelopes (`ApiResponse<T>`, `PaginatedApiResponse<T>`) | Must be registered with a **concrete** entity type in your aggregator — that's why core can't pre-register them. |

→ Endpoint contract being described: [api-reference.md](api-reference.md).
