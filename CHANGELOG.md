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
