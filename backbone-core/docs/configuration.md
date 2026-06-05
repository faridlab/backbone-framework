# ⚙️ Backbone Core — Configuration

**Status:** ✅ Current · **Last Updated:** 2026-06-06

How to compile the crate for your needs and tune its runtime behaviour.

## 🧩 Cargo feature matrix

All features are **off by default** — the default build pulls no optional dependencies.

| Feature | Enables (deps) | Adds to the API | Use when |
|---------|----------------|-----------------|----------|
| *(default)* | — | Traits, HTTP handler types, `InMemoryRepository` | Tests, prototypes, trait consumers |
| `database` | `sqlx` | (alias of `postgres`) | Generic DB wiring |
| `postgres` | `sqlx` (postgres) | `PostgresRepository`, `PostgresRepositoryBuilder`, `PostgresEntity` | Production Postgres storage |
| `prost` | `prost-types` | Protobuf well-known types for gRPC | gRPC services |
| `openapi` | `utoipa` | `ToSchema` on HTTP types, `openapi::BackboneComponents` | Generating an OpenAPI/Swagger spec |
| `full` | `postgres` + `prost` | Both of the above | Full persistence + gRPC |

```bash
cargo build                          # default
cargo build --features postgres
cargo build --features openapi
cargo build --features "full openapi"
```

Notes:
- `full` intentionally does **not** include `openapi` — schema generation is a build-time
  concern most services opt into explicitly.
- `openapi` is purely additive: it changes no runtime behaviour and adds nothing to the
  default dependency graph (`cargo tree -e no-dev` shows no `utoipa` without the feature).

## 🎛️ Runtime knobs

### Mount path

`BackboneCrudHandler::routes(service, base_path)` takes the `base_path` as a runtime
string. Convention is `/api/v1/{collection}`, e.g. `/api/v1/products`. Use
`read_routes` / `write_routes` to mount the read and write halves separately (for
example behind different middleware or auth).

### Limits (public constants)

These are compile-time constants in the crate, enforced inside the handlers/service.
They are exported so callers (including gRPC/internal callers) can reason about them.

| Constant | Value | Effect |
|----------|-------|--------|
| `MAX_PER_PAGE` | 100 | `limit` is clamped here before the offset is computed |
| `MAX_PAGINATION_OFFSET` | 10_000 | Deeper paging → `400` instead of a slow deep `OFFSET` scan (~100 pages at max size) |
| `MAX_BATCH_SIZE` | 1_000 | Larger batch requests → `400`, at both the HTTP and service layer |

If you regularly need to page past the offset cap, add filters to narrow the result set
(the `400` message says exactly this) — deep offset scans are the thing the cap exists
to prevent.

## 🗂️ Application configuration (`config` module)

Beyond the CRUD knobs above, `backbone-core` ships a full application configuration
system (`backbone_core::config`) used by services built on the framework:

- `BackboneConfig` — top-level config: `app`, `server`, `database`/`cache` maps,
  `modules`, `logging`, `monitoring`, `contexts`, `features`, `security`.
- Loaders: `BackboneConfig::from_file(path)` (YAML / TOML / JSON, with environment-
  variable substitution) and `from_env()`.
- `ConfigurationBus` — broadcasts `ConfigChangeEvent`s for hot-reload-aware modules.

See the rustdoc for `backbone_core::config` for the full surface. For per-endpoint
behaviour, see [api-reference.md](api-reference.md); for enabling OpenAPI, see
[openapi.md](openapi.md).
