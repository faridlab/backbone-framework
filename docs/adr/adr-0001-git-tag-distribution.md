# ADR-0001: Distribute via git tags, not crates.io

- **Status:** Accepted
- **Date:** 2026-04-24
- **Deciders:** Backbone maintainers

## Context

Backbone is a workspace of 17 crates consumed by internal services and modules.
The crates evolve together and are frequently changed as a set. Publishing 17
crates to crates.io on every change imposes real cost: version coordination across
interdependent crates, irreversible published versions, name-squatting exposure,
and public availability of code that is currently internal. Consumers, meanwhile,
need a way to depend on a **consistent snapshot** of the whole framework.

## Decision

We will distribute Backbone as **git dependencies pinned to a release tag**, not
via crates.io. Consumers depend on it with
`{ git = "…/backbone-framework", tag = "v<version>" }`, pinning **every** member
crate they use to the **same** tag. Releases are cut by pushing a `v<version>` git
tag; the release workflow verifies the tag's version matches
`[workspace.metadata.release].version` before publishing a GitHub Release.

## Alternatives considered

- **Publish to crates.io** — rejected: forces public availability, per-crate
  version juggling, and irreversible releases for code that changes as a set.
- **Depend on `branch = "main"`** — rejected: every `cargo update` pulls HEAD,
  silently dragging in breaking changes. Explicitly warned against in the README.
- **Vendored copies per consumer** — rejected: loses a single source of truth and
  makes upgrades manual.

## Consequences

- **Easier:** releasing (push a tag), keeping a consumer at one consistent
  snapshot, keeping the code internal.
- **Harder:** consumers must pin all backbone crates to the *same* tag by hand;
  mismatched tags produce inconsistent builds. There is no crates.io discovery or
  docs.rs.
- **We live with:** the release workflow as the guard that a tag's version matches
  the manifest — a `v*` tag whose version disagrees is refused, by design.
