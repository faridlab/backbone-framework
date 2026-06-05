# 📚 Backbone Core — Documentation

**Status:** ✅ Current
**Crate:** `backbone-core` · **Last Updated:** 2026-06-06

`backbone-core` is the foundation crate of the Backbone Framework. It turns a single
entity + its DTOs + a repository into a complete, consistent CRUD service — ~21 REST
endpoints (and the matching gRPC surface) — without per-entity boilerplate. It also
carries the framework's building blocks: services, repositories, validation, policies,
DDD patterns (aggregates, value objects, specifications), CQRS, configuration, and
optional OpenAPI schema generation.

## 🗺️ Start here

| If you want to… | Read |
|---|---|
| Understand how the crate is put together | [architecture.md](architecture.md) |
| Wire up CRUD for your own entity, with examples | [usage.md](usage.md) |
| Look up an endpoint, query param, or response shape | [api-reference.md](api-reference.md) |
| Choose feature flags and tune runtime limits | [configuration.md](configuration.md) |
| Generate / serve an OpenAPI (Swagger) spec | [openapi.md](openapi.md) |
| A copy-paste OpenAPI 3.1 skeleton | [openapi.template.yaml](openapi.template.yaml) |

New users → start with **usage.md**. Contributors → start with **architecture.md**.

## 🧩 Feature flags

`backbone-core` is plumbing: optional capabilities are behind Cargo features so you only
pull what you need. All features are **off by default**.

| Feature | Pulls in | Use when |
|---------|----------|----------|
| *(default)* | axum, serde, tokio, … | In-memory repositories, traits, HTTP handler types |
| `database` / `postgres` | `sqlx` (postgres) | Production Postgres-backed repositories |
| `prost` | `prost-types` | gRPC / Protobuf well-known types |
| `openapi` | `utoipa` | Derive `ToSchema` on the HTTP types + `BackboneComponents` |
| `full` | `postgres` + `prost` | Everything persistence/gRPC (does **not** include `openapi`) |

```bash
cargo build                       # default: no optional deps
cargo build --features postgres   # Postgres repositories
cargo build --features openapi    # OpenAPI schema generation
cargo build --features "full openapi"
```

## 🔗 Related

- Crate root `README.md` — feature overview and the endpoint table.
- `examples/` — runnable examples (`basic_usage`, `advanced_pagination`, `error_handling`, `scenario_ecommerce`).
- Root `CHANGELOG.md` — monorepo-wide changelog.
