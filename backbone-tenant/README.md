# backbone-tenant

Per-tenant runtime registry for database-per-tenant isolation ([ADR-0006](../../docs/handbook/adr/0006-per-tenant-database-isolation.md)).

ADR-0005 makes the tenant the **database**. Modules bind their pool at build time, so this crate
builds a tenant's whole runtime (pool + module graph) **once** and caches it, keyed by tenant id —
resolving each request to its tenant's resources without rewriting any module.

- `TenantRuntimeFactory` — the composition root implements this: open a pool to the tenant's DB, build its modules.
- `TenantRegistry` — bounded, lazy, LRU-evicted cache. Build-once under concurrency; failures don't poison; eviction never kills in-flight requests.

Generic over the runtime type, so the routing logic is fully unit-tested without a database. Depends
only on `tokio` sync + `thiserror` — never on `sqlx` or a domain module.

## Provisioning (feature `provision`)

`TenantProvisioner` (Seam A) creates and migrates a tenant's database: `CREATE DATABASE tenant_<slug>`,
open a connection, run the migration SQL, and — when a tenant is retired — `DROP DATABASE`. Because
`CREATE DATABASE` cannot parameterize its identifier, tenant ids are strictly validated to a bounded
`[a-z0-9_-]` slug and rejected otherwise, so an injection-shaped id is an error rather than an escaped
string. Off by default (it pulls `sqlx`); enable it in the composition root that provisions tenants.
Verified end-to-end against a real Postgres (`tests/provision_live.rs`, gated on
`BACKBONE_TENANT_ADMIN_DSN`).
