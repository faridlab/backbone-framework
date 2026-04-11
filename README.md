# Backbone Framework

A modular Rust framework for building production-grade backend services. Backbone is organized as a Cargo workspace of focused, independently usable crates — each crate owns a single concern (persistence, auth, queues, search, …) and can be composed to form a complete service.

## Philosophy

- **Lift-and-shift discipline.** Each crate is self-describing: dependencies are declared per-crate, not via `[workspace.dependencies]`. This preserves the property that any crate can be extracted and used on its own.
- **Protocol-agnostic core.** Domain primitives (CQRS, flows, state machines, policies) live in `backbone-core` and are independent of HTTP/gRPC/GraphQL transports.
- **Pluggable backends.** Every infrastructure crate ships multiple implementations behind a trait (e.g. Redis/Memory for cache, S3/MinIO/Local for storage, SMTP/SES/Mailgun for email).
- **Standard 11 endpoints.** Entities built on `backbone-core` automatically expose a consistent CRUD surface across HTTP and gRPC.

## Workspace Crates

| Crate | Purpose |
|---|---|
| [backbone-core](backbone-core/) | Domain primitives: CQRS, flows, state machines, policies, config, module registry, and the 11 standard CRUD endpoints |
| [backbone-orm](backbone-orm/) | Repository pattern, query builder, filtering, migrations, seeding, raw queries |
| [backbone-auth](backbone-auth/) | Authentication: JWT, password hashing, sessions, audit, middleware |
| [backbone-authorization](backbone-authorization/) | Authorization: policies, permission cache, RBAC middleware |
| [backbone-cache](backbone-cache/) | Caching abstraction with Memory and Redis backends |
| [backbone-storage](backbone-storage/) | Object storage with S3, MinIO, and Local backends; compression and security scanning |
| [backbone-email](backbone-email/) | Transactional email with SMTP, SES, and Mailgun providers |
| [backbone-messaging](backbone-messaging/) | Event bus, integration bus, CRUD event envelopes |
| [backbone-queue](backbone-queue/) | Job/message queues with Redis, RabbitMQ, and SQS; FIFO, dedupe, compression, monitoring |
| [backbone-jobs](backbone-jobs/) | Scheduled jobs: cron, pg_cron, in-memory and persistent storage |
| [backbone-search](backbone-search/) | Search abstraction with Elasticsearch and Algolia backends |
| [backbone-graphql](backbone-graphql/) | GraphQL helpers: pagination, error mapping |
| [backbone-rate-limit](backbone-rate-limit/) | Rate limiting with Redis storage and HTTP middleware |
| [backbone-health](backbone-health/) | Health checks and readiness/liveness endpoints |
| [backbone-observability](backbone-observability/) | Logging, tracing, metrics, and middleware |

## Getting Started

Add the crates you need to your service's `Cargo.toml`:

```toml
[dependencies]
backbone-core = { path = "../backbone-framework/backbone-core" }
backbone-orm  = { path = "../backbone-framework/backbone-orm" }
backbone-auth = { path = "../backbone-framework/backbone-auth" }
```

Each crate ships with its own `README.md` and `examples/` directory — start there for usage patterns.

## Building

```bash
# Build the entire workspace
cargo build

# Build a single crate
cargo build -p backbone-core

# Run tests for a single crate
cargo test -p backbone-orm

# Run all workspace tests
cargo test
```

## Repository Layout

```
backbone-framework/
├── Cargo.toml              # workspace manifest
├── backbone-core/          # domain primitives + CRUD foundation
├── backbone-orm/           # persistence layer
├── backbone-auth/          # authentication
├── backbone-authorization/ # authorization
├── backbone-cache/         # caching
├── backbone-storage/       # object storage
├── backbone-email/         # transactional email
├── backbone-messaging/     # event/integration bus
├── backbone-queue/         # message queues
├── backbone-jobs/          # scheduled jobs
├── backbone-search/        # search engines
├── backbone-graphql/       # GraphQL helpers
├── backbone-rate-limit/    # rate limiting
├── backbone-health/        # health checks
└── backbone-observability/ # logging, tracing, metrics
```

## License

See individual crate metadata for license information.
