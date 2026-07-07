<!-- Reader: Evaluator · Mode: Explanation -->
# Background & Prior Art

Backbone did not appear in a vacuum. This page credits what came before, says
plainly what Backbone borrows, and — without strawmanning — why the existing
options did not fit the problem it set out to solve.

## The problem being solved

A team standing up several Rust backend services faces the same wall each time:
Axum gives you a router and nothing else; SQLx gives you queries and nothing
else. Everything between "here is a Postgres pool" and "here is a consistent,
paginated, soft-deleting, bulk-capable REST **and** gRPC surface for fifty
entities" is left to you. Written once per service, that layer drifts: entity A
paginates one way, entity B another; one service soft-deletes, the next hard-deletes.

Backbone is the extraction of that layer into something shared and generated, so
it is written *once* and identical *everywhere*.

## Prior art, and what Backbone takes from it

### Laravel / Rails — convention over configuration

The idea that a framework should hand you a consistent resource surface for free
comes straight from the Rails/Laravel lineage. Backbone borrows the **service
provider / module registration** pattern almost verbatim — [`backbone-core`](../backbone-core/)
calls it a "Laravel-style service provider pattern" in its own module system.

*What it rejects:* the dynamic, reflection-heavy runtime. Backbone is Rust — the
consistency is delivered through generics and macros checked at compile time,
not runtime metaprogramming. You get the convention without giving up the type
system.

### Django REST Framework / API Platform — generated CRUD

The notion that declaring an entity should yield a full paginated, filterable,
sortable REST surface is DRF's and API Platform's core value. Backbone takes the
same promise — the **11 standard endpoints** — and extends it with soft-delete +
trash lifecycle, atomic bulk operations, and a *second* protocol (gRPC) from the
same definition.

*What it rejects:* being HTTP-first. In DRF the web layer is the center of
gravity; in Backbone the [protocol-agnostic core](philosophy.md) is, and HTTP is
one adapter among several.

### Spring Boot — batteries included, one runtime

Spring proved teams will trade portability for a rich, cohesive platform.
Backbone wants the richness but not the trade: the **lift-and-shift discipline**
(self-describing crates, no workspace-wide dependency inheritance) is a direct
reaction to how hard it is to extract one Spring component from the platform it
assumes around it.

### Rust building blocks it stands on — not reinvented

Backbone is emphatically *not* a from-scratch stack. It composes the established
Rust ecosystem: **Axum** (HTTP), **Tonic/Prost** (gRPC), **SQLx** (async
Postgres), **Tokio** (runtime), **Serde** (serialization), **thiserror** (library
errors), and optionally **utoipa** (OpenAPI). See [Technology](technology.md) for
the rationale behind each. Backbone's contribution is the *layer above* these —
the generic CRUD/CQRS/lifecycle machinery that ties them into one consistent
surface — not the primitives themselves.

## Why not just use those tools directly?

| If you reach for… | You still have to hand-build… | Backbone gives it to you |
|-------------------|-------------------------------|--------------------------|
| Axum + SQLx alone | Pagination, filtering, soft-delete, bulk, per-entity consistency | The standard endpoint set, generated |
| A Python/JVM framework | A Rust rewrite for performance/footprint | Native Rust, compile-time-checked |
| A heavier Rust framework | An exit strategy when you outgrow it | Extractable, self-describing crates |

## The direct ancestor: `monorepo-backbone`

Backbone is not a greenfield project — it is the **extraction of an internal
monorepo** (`monorepo-backbone`) into an independent, releasable workspace. The
member crates were lifted out **byte-for-byte** at the `2.0.0` release
([CHANGELOG](../CHANGELOG.md)), which is precisely why the lift-and-shift
discipline is treated as sacred: the workspace's whole reason to exist is that
the extraction was, and remains, clean. `backbone-graphql` and `backbone-jobs`
were the first crates *born* in this workspace rather than lifted into it.

## Where Backbone sits in the wider Metaphor world

This workspace is one project (`type: crate`) inside the larger **Metaphor**
meta-workspace, alongside domain `module` projects (accounting, catalog,
inventory, …), a runnable `backend-service`, and the `metaphor` CLI. Backbone is
the **plumbing tier** the modules and services build on. That separation — plumbing
here, domain logic in modules — is the same non-goal stated in the
[Philosophy](philosophy.md): crates never hold business rules.
