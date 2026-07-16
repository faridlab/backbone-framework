# backbone-tenant

Per-tenant runtime registry for database-per-tenant isolation ([ADR-0006](../../docs/handbook/adr/0006-per-tenant-database-isolation.md)).

ADR-0005 makes the tenant the **database**. Modules bind their pool at build time, so this crate
builds a tenant's whole runtime (pool + module graph) **once** and caches it, keyed by tenant id —
resolving each request to its tenant's resources without rewriting any module.

- `TenantRuntimeFactory` — the composition root implements this: open a pool to the tenant's DB, build its modules.
- `TenantRegistry` — bounded, lazy, LRU-evicted cache. Build-once under concurrency; failures don't poison; eviction never kills in-flight requests.

Generic over the runtime type, so the routing logic is fully unit-tested without a database. Depends
only on `tokio` sync + `thiserror` — never on `sqlx` or a domain module.
