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
back to `## [Unreleased]`.

## [Unreleased]

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
