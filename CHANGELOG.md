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
