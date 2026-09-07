# Changelog

All notable changes to this workspace are documented here. Format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/); versions adhere to
[Semantic Versioning](https://semver.org/spec/v2.0.0.html).

Because this framework is distributed as git deps (not crates.io), the version
below is the **monorepo version** — the same number applies to every member
crate at this commit. Downstream projects pin the whole framework with
`{ git = "...", tag = "v<version>" }`.

The release workflow reads the section matching the git tag's version and
uses it as the GitHub Release body. If no matching section is found it falls
back to the `## [Unreleased]` section.

## [Unreleased]

## [2.7.15] - 2026-09-07

### Fixed
- Test infrastructure: the tenant-router live proof compiles again — its `OrgClaims` struct
  literal predates the token-purpose (`typ`) field added in 2.7.14 and was missing the member
  (E0063). The proof's minted tokens are deliberately typless: the typless-legacy acceptance is
  part of what the proof carries. The 2.7.14 tag was published with this file broken (its
  release lane failed before publishing); pin 2.7.15 instead.

## [2.7.14] - 2026-09-07

### Added
- `backbone-auth`: the org session issuer (`backbone_auth::org::OrgIssuer`, feature `axum`) — the
  mint twin of `OrgVerifier` (ADR-0027/0028). An org session is born in exactly one place: the
  issuing service resolves the acting unit and entitlements from the tenant's own data
  (membership tables, the org spine) and seals them into a signed token; a client never states
  its org unit. `org_unit_id` is a required argument, mirroring the guard's 401-on-absent claim —
  a token that cannot name its node is never minted. HS256 and RS256 constructors;
  `issue_access`/`issue_refresh` mint the pair with a `typ` claim discriminating them.
- Test infrastructure: a live end-to-end proof of the issuance chain (`backbone-auth`
  `tests/org_session_live.rs`, gated on `BACKBONE_AUTH_ORG_DSN` against a disposable database).
  It builds the minimal org spine, mints with `OrgIssuer`, and drives the real guard through
  `tower::ServiceExt::oneshot`: the access token passes and the handler's fenced read returns
  exactly the entitlement-union rows (subtree + entitled sister + root, parent company
  excluded), the refresh twin of the same session is 401, an access token naming a unit the
  tree does not hold is 403, and no token is 401. The no-database contract for guard and issuer
  round-trips lives in `tests/org_guard.rs`.

### Fixed
- `backbone-auth`: token-purpose (`typ`) discipline on both HTTP guards. `CompanyClaims` and
  `OrgClaims` now read an optional `typ` claim, and a present value other than `"access"` fails
  verification: a refresh token — valid signature, longer life — could previously open a scoped
  session on `company_auth`/`org_auth` routes. Tokens minted before `typ` existed carry no claim
  and stay accepted.

## [2.7.13] - 2026-09-07

### Added
- `backbone-tenant`: host→tenant resolution (`backbone_tenant::resolve`). `HostResolver` maps a
  request's host to its tenant — `{slug}.<product-domain>` subdomains, an optional custom-domain
  map for enterprise domains, and a development-only header override that a production resolver
  refuses loudly (403) rather than ignoring. The slug law matches the provisioner's
  database-name law, so every resolved tenant id is safe to interpolate downstream.
- `backbone-tenant`: the tenant-routing middleware (feature `axum`, `backbone_tenant::axum_router`).
  `tenant_route` resolves the request's tenant, gets-or-builds its runtime through
  `TenantRegistry`, and attaches the runtime — and, via the `ProvidesDatabase` hand-off, its
  tenant-dedicated `PgPool` — to the request extensions. A host that names no tenant is 404 and a
  tenant whose database cannot be built is 503; there is no fallback tenant.
- `backbone-auth`: org-tree session guard (feature `axum`, `backbone_auth::org`). The org twin of
  `company_auth`: the acting org unit comes off a signed Bearer token (`org_unit_id` required,
  `entitled_units` optional), the session's scope is resolved over the request's tenant pool (the
  one `tenant_route` inserted), and the whole downstream handler runs inside
  `with_org_request_scope`. A token naming a unit outside this tenant's tree is 403 — identity
  without tenancy is not access.
- `backbone-tenant`: `PgPoolFactory::connect_as` — connect tenant runtime pools as a
  least-privilege login instead of the provisioning admin. Without it the pool connects as the
  admin user, and superusers bypass row-level security: the fence would enforce nothing.
- Test infrastructure: a live end-to-end proof (`backbone-tenant` `tests/router_live.rs`, gated on
  `BACKBONE_TENANT_ROUTER_DSN`) — two real tenant databases provisioned through
  `TenantProvisioner`, one axum app over both, asserting per-tenant fence visibility, the
  cross-tenant 403, entitlement-union widening, and fail-closed routing/auth.

### Fixed
- Test infrastructure: the live RLS/org-scope suites mint a per-run database role instead of a
  fixed name — a fixed role breaks every later run on shared dev clusters (`DROP ROLE` fails while
  the role holds grants in another database). The release lane runs against a Postgres service so
  the workspace's live suites (outbox mechanics/relay/migrations, ORM fence proofs, tenant
  provisioning) actually execute before a release is published.

## [2.7.12] - 2026-09-07

### Changed
- `backbone-outbox`: **breaking** — `OutboxRecord::new` now takes a required `company_id: Uuid`
  (4th arg), the `stage()` INSERT + `migrate()` table template carry a `company_id` column, and the
  relay reads it. This lets each module's `outbox_events` table be fenced by tenant (ADR-0011).
  Callers must pass the owning company; modules that don't bump to this tag are unaffected (they pin
  the older tag).
- `backbone-core`: re-export `PaginatedApiResponse`, `BatchIdsRequest`, `BulkUpdateItem`,
  `BulkPatchItem`, and `BulkPatchRequest` from the crate root (previously only reachable
  via `backbone_core::http::…`).

### Added
- `backbone-orm`: org-tree request scope (`backbone_orm::org_scope`) for the entitlement-union RLS
  fence. `resolve_org_scope` derives a session's entitled org-unit ids from the organization spine
  (union of subtrees + the root node) plus the legacy governing company id;
  `with_org_request_scope` carries both fence variables (`app.scope_unit_ids`, `app.company_id`)
  on a request-dedicated connection for the whole request and clears them before pool return;
  `bind_org_scope_on` is the transaction-local twin; `execute_unit_scoped` is the statement-level
  fence for single-node writes outside a request scope. Generated repositories need no changes —
  the scope binds the same request connection the existing scoped helpers route onto.
- `backbone-core`: relation expansion on read endpoints. `list` and `get_by_id`
  accept `?include=<rel>` (alias `?with=`) to hydrate declared to-one relations,
  injecting each related row as a sibling object keyed by the relation name.
  Comma-separated; only relations an entity declares are honored and unknown
  names are ignored. Expansion is **batched** — one `WHERE id = ANY(...)` per
  relation across all rows in the page (no N+1) — and the target table is taken
  from generator-emitted metadata, never client input, so it is not an injection
  vector. Entities opt in via a new defaulted `backbone_orm::EntityRepoMeta::relations()`
  hook returning `(relation_name, target_table, local_fk_field)` tuples; the
  generated Postgres repos hydrate via `backbone_orm::fetch_by_ids_as_json`
  (newly exported). Expanded objects have their top-level keys camelCased to read
  consistently with the response. Runs **after** field-security and **before**
  sparse projection. *v1 limitation:* the expanded object is the raw related row,
  **not** run through the target's response DTO or its `@private` field-security —
  do not enable `?include=` for a target that has private fields. Defaults are
  no-ops, so entities that don't override `relations()` are unaffected.
- `backbone-core`: field-level security (`@private` / `@owner`) on the read
  endpoints (`list`, `get_by_id`, `list_deleted`, `get_deleted_by_id`).
  `backbone-orm::EntityRepoMeta` gains two defaulted hooks — `private_fields()`
  (response JSON keys, camelCase, visible only to the row owner or platform/root)
  and `owner_field()` (the response key holding the owner/tenant id). A new
  `backbone_core::AccessScope` enum (`Platform` | `Tenant(String)`) is read from
  an axum `Extension`, injected by the application's auth middleware: `Platform`
  sees every field; `Tenant(id)` sees `@private` fields only when the row's
  `@owner` equals `id`; an absent scope **fails closed** (private fields stripped).
  Security runs **before** sparse projection, so the visibility ceiling always
  beats a `?fields=` request. Defaults are no-ops — entities that don't override
  the hooks behave exactly as before. (`backbone-orm` is now a normal dependency
  of `backbone-core`, not dev-only.)
- `backbone-core`: optional OpenAPI/Swagger support behind a default-off `openapi`
  feature. Enabling it derives `utoipa::ToSchema` on the shared HTTP envelope/request
  types (`ApiResponse`, `PaginatedApiResponse`, `PaginationResponse`, `BulkResponse`,
  `BulkCreateRequest`, `UpsertRequest`, `BatchIdsRequest`, `BulkUpdateItem`,
  `BulkPatchItem`, `BulkPatchRequest`, `FilterOptions`, `SortOrder`, `ListQueryParams`,
  `ListRequest`, `PaginationRequest`) and exposes `openapi::BackboneComponents`, a
  reusable `utoipa::OpenApi` document of the non-generic component schemas that
  downstream entity crates merge into their own spec. The default build is unchanged —
  no `utoipa` in the dependency graph without the feature.
- `backbone-core`: a multi-file `docs/` set — `architecture.md`, `usage.md`,
  `api-reference.md` (all 21 endpoints, query grammar, response envelopes, status
  codes), `configuration.md`, `openapi.md` (utoipa + Swagger UI guide), an index
  `README.md`, and a copy-paste `openapi.template.yaml` (OpenAPI 3.1) for teams not
  wiring utoipa.
- `backbone-core`: sparse fieldsets on read endpoints. `list`, `get_by_id`,
  `list_deleted`, and `get_deleted_by_id` accept `?fields=a,b,c` to project each
  response object down to the requested top-level keys plus the always-on `id`,
  letting clients trim response payloads. Field names are comma-separated and
  whitespace-trimmed; unknown keys are ignored and an absent/empty value returns
  every field. `fields`, `include`, and `with` are reserved response-shaping
  query keys — stripped from the filter map before it reaches the repository, so
  they never leak into the `WHERE` clause.

### Fixed
- `backbone-orm`: test target builds again on current chrono (`NaiveDateTime::from_timestamp_opt`
  was removed upstream; the query-builder tests now use `DateTime::from_timestamp`).

## [2.3.0]

### Added
- `backbone-core`: atomic batch operations across the CRUD stack (HTTP handlers,
  `CrudService`, persistence traits, the `impl_crud_repository!` macro, and
  `backbone-orm::GenericCrudRepository`). Each runs inside a single transaction
  with all-or-nothing semantics — if any id is missing or already in the target
  state, the whole batch is rolled back and no rows are written. New endpoints:
  - `PUT  {collection}/bulk` — full-update many (`[{ "id", ...fields }]`)
  - `PATCH {collection}/bulk` — partial-update many; accepts a shared
    `{ ids, patch }` shape or a per-id `{ items: [{ id, patch }] }` shape
    (auto-detected via untagged deserialization)
  - `POST {collection}/delete/bulk` — soft-delete many by id (`{ "ids": [...] }`)
  - `POST {collection}/restore/bulk` — restore many soft-deleted by id
  - `POST {collection}/restore/all` — restore every soft-deleted entity
  - `DELETE {collection}/trash/bulk` — permanently delete many trashed by id
- `backbone-core`: `MAX_BATCH_SIZE` (1,000) public constant, enforced at both the
  HTTP layer (`400 Bad Request`) and the service layer (so gRPC / internal callers
  get the same bound). Lifecycle hooks and CRUD events fire per affected entity,
  mirroring the single-row methods — including `restore_all`, which now emits a
  `Restored` event per restored entity.
- `backbone-core`: batch behaviour hardening — id-list operations (soft-delete,
  restore, permanent-delete) de-duplicate ids before the `IN (...)` query so a
  repeated id no longer fails the whole batch; bulk update / partial-update reject
  a request that maps the same id twice with a clear `400`; `bulk_permanent_delete`
  relies on the repository's atomic in-trash check rather than a redundant,
  race-prone per-id pre-validation pass.

## [2.2.2]

### Changed
- `backbone-core`: list/query CRUD handlers (including `list_deleted`) now reject
  requests that page beyond a fixed offset depth with `400 Bad Request` instead
  of running an expensive deep `OFFSET` scan. The page size is clamped to
  `MAX_PER_PAGE` (100) before computing the offset, and any request whose
  effective offset exceeds `MAX_PAGINATION_OFFSET` (10,000 — ~100 pages at max
  size) is reported as `Result set too deep: offset ... exceeds the maximum of
  10000. Please add filters to narrow your search.` `MAX_PER_PAGE` and
  `MAX_PAGINATION_OFFSET` are exposed as public constants.

## [2.2.1]

### Changed
- `backbone-core`: list/query CRUD handlers now return `400 Bad Request` instead
  of `500 Internal Server Error` when a request supplies an unknown filter or
  sort key. Such params are injected as column names, so a typo or stray
  camelCase param (e.g. `sortOrder`) surfaces as a Postgres
  `column "..." does not exist` (SQLSTATE 42703) or `invalid input syntax`
  error — these are now classified as client faults and reported as
  `Invalid query parameter or filter: ...`.

## [2.2.0]

### Added
- `backbone-core`: `extractors::JsonOrForm<T>` request body extractor that accepts
  either `application/json` or `application/x-www-form-urlencoded` bodies,
  defaulting to JSON when the `Content-Type` is absent or unrecognized.

### Changed
- `backbone-core`: `BackboneCrudHandler` now decodes create, update, partial
  update, upsert, and bulk-create bodies via `JsonOrForm`, so all generated CRUD
  endpoints accept JSON **and** form-encoded payloads with no handler changes.

## [2.0.0]

### Release model
- Introduce monorepo versioning (`[workspace.metadata.release].version`) and
  tag-driven releases (`v<version>`). Downstream consumers should pin via
  `tag = "v2.0.0"` instead of `branch = "main"`.

### Contents at this release
- `backbone-auth`, `backbone-authorization`, `backbone-cache`, `backbone-core`,
  `backbone-email`, `backbone-health`, `backbone-messaging`,
  `backbone-observability`, `backbone-orm`, `backbone-queue`,
  `backbone-rate-limit`, `backbone-search`, `backbone-storage` — extracted
  from `monorepo-backbone` as independent member crates; source byte-for-byte
  unchanged from the monorepo.
- `backbone-graphql`, `backbone-jobs` — new in this workspace.
