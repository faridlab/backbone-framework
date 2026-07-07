# ADR-0004: One version for the whole workspace

- **Status:** Accepted
- **Date:** 2026-04-24
- **Deciders:** Backbone maintainers

## Context

With 17 interdependent crates released together as git tags
([ADR-0001](adr-0001-git-tag-distribution.md)), per-crate semantic versions add
coordination cost and consumer confusion: which crate's version is "the release"?
The crates were extracted with their own `version` fields frozen (many still read
`2.0.0` from the byte-for-byte extraction), so those fields cannot serve as the
release number without being churned on every change.

## Decision

We will use **monorepo versioning**: a single version applies to every member
crate at a given commit, authoritative in **one place** —
`[workspace.metadata.release].version` in the workspace
[`Cargo.toml`](../../Cargo.toml). The release tag `v<version>` and this field move
together; the release workflow refuses a tag whose version disagrees. Semver is
interpreted **for the workspace as a whole**: a major bump means a breaking change
exists *somewhere* in the workspace, not necessarily in the crate a given consumer
uses. Per-crate `version` fields are not the release source of truth.

## Alternatives considered

- **Independent per-crate semver** — rejected: heavy coordination, and consumers
  pinning by tag get a snapshot anyway, so per-crate versions add confusion, not
  clarity.
- **Version only the crates that changed** — rejected: requires per-crate change
  detection and undermines the "one consistent snapshot" model of git-tag
  distribution.

## Consequences

- **Easier:** cutting a release (one number, one tag), reasoning about "what
  version am I on" (the tag).
- **Harder:** a consumer cannot tell from the version *which* crate changed; they
  read the [CHANGELOG](../../CHANGELOG.md), which is scoped per crate for exactly
  this reason.
- **We live with:** stale-looking per-crate `version` fields — intentional, and
  documented in the Maintainer guide so no one "fixes" them into a false source of
  truth.
