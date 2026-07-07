# ADR-0003: Protocol-agnostic core with pluggable backends

- **Status:** Accepted
- **Date:** 2026-04-24
- **Deciders:** Backbone maintainers

## Context

A CRUD framework can be built HTTP-first, with the web layer at the center and the
domain logic reachable only through it. That is the common shape (DRF, many Rails
resources) and it is fast to start with. But Backbone's entities need to be served
over **both HTTP and gRPC** (and, in places, GraphQL), and its infrastructure
needs to run against different backends in dev vs prod (in-memory vs Postgres,
Memory vs Redis, Local vs S3). If the domain code imports a web framework or a
concrete database, neither of those is possible without duplication.

## Decision

We will keep the **domain primitives in `backbone-core` protocol-agnostic** —
CQRS, flows, state machines, policies, aggregates, and the `CrudService` trait
know nothing about HTTP/gRPC/GraphQL. Transports are **adapters** (`http`, `grpc`,
`graphql`) layered on top; the same `CrudService` implementation backs all of
them. Likewise, every infrastructure crate exposes a **trait** and ships multiple
implementations behind it, selected by config/feature — never by a code change in
the consumer. Databases and other heavy backends are gated behind Cargo features
so the core compiles without them.

## Alternatives considered

- **HTTP-first design** — rejected: would require a parallel gRPC implementation
  and couple domain code to Axum.
- **One hard-wired backend per concern** — rejected: kills the dev/prod backend
  swap and the "any crate usable on its own" goal.
- **Runtime plugin/reflection** — rejected: Rust favors compile-time generics and
  traits; reflection would sacrifice the type-safety that is the point of using
  Rust.

## Consequences

- **Easier:** serving one entity over multiple protocols; swapping backends by
  config; testing against in-memory backends.
- **Harder:** more indirection (a trait boundary between domain and transport /
  storage); adding a transport means writing an adapter, not editing the core.
- **We live with:** the discipline that domain code must never reach for a
  transport or a concrete backend — enforced in review as a layering rule.
