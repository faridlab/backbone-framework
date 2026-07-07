<!-- Reader: Evaluator + Maintainer · Mode: Explanation -->
# Technology & the "Why"

Every dependency here is load-bearing and chosen for a reason. This page gives
the reasoning behind the stack: each choice gets a one-line rationale and the
named alternative it beat. Versions are those pinned in
[`backbone-core/Cargo.toml`](../backbone-core/Cargo.toml) at the time of writing.

## The stack at a glance

| Concern | Choice | Version | Rejected alternative |
|---------|--------|---------|----------------------|
| Language | Rust (edition 2021) | — | Go, Kotlin/JVM |
| Async runtime | Tokio | `1.x` | async-std |
| HTTP transport | Axum + Tower | `0.7` / `0.4` | actix-web, warp |
| gRPC transport | Tonic + Prost | `0.12` / `0.13` | grpcio |
| Database | SQLx (Postgres, tokio-rustls) | `0.8` | Diesel, SeaORM |
| Serialization | Serde (+ serde_json, serde_yaml, toml) | `1.0` / `0.9` / `0.8` | miniserde, hand-rolled |
| Identifiers | uuid (v4) | `1.x` | auto-increment, ULID |
| Time | chrono | `0.4` | time |
| Library errors | thiserror | `1.0` | anyhow (kept for bins only) |
| Async traits | async-trait | `0.1` | native AFIT |
| Validation | regex | `1.x` | validator |
| Observability | tracing | `0.1` | log |
| OpenAPI (optional) | utoipa | `5` (feature `openapi`) | paperclip, hand-written spec |

## The reasoning

### Rust, edition 2021 — *the whole premise*
Native performance and a small footprint with compile-time guarantees. The
consistency Backbone promises (identical CRUD surface across many entities) is
delivered through **generics and macros checked by the compiler**, not runtime
reflection. *Rejected:* Go (no generics-driven type-safe CRUD at the time the
lineage was set) and JVM frameworks (footprint, and the very portability tax
Backbone was built to escape).

### Tokio — the async substrate
`features = ["full"]`. It is the de-facto async runtime and everything below
(Axum, Tonic, SQLx-rustls) is built for it, so choosing anything else would fight
the ecosystem. *Rejected:* async-std, which the surrounding libraries do not
target.

### Axum + Tower / tower-http — the HTTP adapter
Axum is Tokio-native, `Router`-based, and composes with the Tower middleware
ecosystem (Backbone uses `tower-http`'s `cors` and `trace` layers). It lets the
generic `BackboneCrudHandler` build a full router from a `CrudService` with no
per-entity handler code. *Rejected:* actix-web (its own actor runtime) and warp
(filter model composes less cleanly with generic handlers).

### Tonic + Prost — the gRPC adapter
The same `CrudService` that backs the HTTP router also backs a gRPC service.
Tonic is the Tokio-native gRPC stack; Prost handles protobuf. `prost-types` is
gated behind the `prost` feature so the default build does not pull it. This is
the **protocol-agnostic core** made concrete: one service definition, two
transports. *Rejected:* grpcio (C-core bindings, heavier build).

### SQLx — persistence, and why it is *optional* in core
SQLx gives async, compile-time-checked queries against Postgres without an ORM's
abstraction ceiling. Crucially, in `backbone-core` it sits behind the `postgres`
/ `database` features — the core compiles with **no database in the graph** so
that the domain primitives stay persistence-free. The real persistence traits
and generic repositories live in [`backbone-orm`](../backbone-orm/). *Rejected:*
Diesel (synchronous, macro-DSL lock-in) and SeaORM (heavier active-record model
that fights the trait-based repository design).

### Serde family — one serialization model, three formats
`serde` + `serde_json` for the wire, `serde_yaml` and `toml` for configuration.
One derive-based model covers HTTP bodies, gRPC-adjacent DTOs, and config files.
The `JsonOrForm` extractor leans on this to accept JSON *and* form-encoded bodies
from the same handler.

### thiserror in libraries, anyhow only in binaries
A framework convention (and a workspace rule): library crates expose typed errors
via `thiserror` so consumers can match on them; `anyhow`'s type-erased errors are
acceptable only in binaries. `backbone-core` carries both because it holds
example/builder code, but its public error surface is typed.

### utoipa — OpenAPI, strictly opt-in
Behind a **default-off `openapi` feature**. Enabling it derives
`utoipa::ToSchema` on the shared HTTP envelope/request types and exposes
`openapi::BackboneComponents`, a reusable component document downstream entity
crates merge into their own spec. The default build carries **no utoipa in the
dependency graph** — you pay for OpenAPI only when you ask for it. Teams not
wiring utoipa can instead copy [`backbone-core/docs/openapi.template.yaml`](../backbone-core/docs/openapi.md).
*Rejected:* baking OpenAPI into every build (violates the "no heavy default
features" rule).

## The cross-cutting rule: features gate the weight

Notice the pattern across the table: **SQLx, prost-types, and utoipa are all
optional.** The `default = []` feature set is deliberately empty. A consumer pulls
only the transports and backends they use. This is the crate-level expression of
the framework's "no heavy default features" principle — see the
[Maintainer guide](maintainer-guide.md) for how to add a capability the same way.

## Deeper reasoning

The most consequential *architectural* technology decisions each have an ADR:

- [ADR-0001](adr/adr-0001-git-tag-distribution.md) — git-tag distribution instead of crates.io.
- [ADR-0002](adr/adr-0002-self-describing-crates.md) — no `[workspace.dependencies]` inheritance.
- [ADR-0003](adr/adr-0003-protocol-agnostic-core.md) — protocol-agnostic core, pluggable backends.
- [ADR-0004](adr/adr-0004-monorepo-versioning.md) — one version for the whole workspace.
