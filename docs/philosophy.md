<!-- Reader: Evaluator · Mode: Explanation -->
# Philosophy & Motivation

**Backbone exists to make the boring 80% of a backend service disappear, without
taking your service hostage to do it.**

Every production backend re-implements the same plumbing: CRUD endpoints,
pagination, soft-delete and trash, auth, caching, queues, health checks,
observability. Most frameworks that offer this plumbing extract a heavy tax —
they own your `main.rs`, dictate your project layout, and couple your domain
code to their runtime so tightly that leaving is a rewrite. Backbone's bet is
that you can get the plumbing for free and still keep your code portable.

## The worldview

Four principles drive every trade-off in this framework. If a design choice
elsewhere in the handbook seems surprising, it almost certainly traces back to
one of these.

### 1. Lift-and-shift discipline

Each crate is **self-describing**. Dependencies are declared per-crate, not
through a shared `[workspace.dependencies]` table. This is deliberate and it is
enforced at the workspace root ([`Cargo.toml`](../Cargo.toml) has no
`[workspace.dependencies]` on purpose). The property it buys: **any single crate
can be lifted out of this workspace and used on its own** with no untangling.
The crates were extracted from an earlier monorepo byte-for-byte, and the
workspace is structured so they can always be extracted again.

*The cost we accept:* some version duplication across crate manifests. We pay it
to keep every crate independently publishable and portable.

### 2. Protocol-agnostic core

Domain primitives — CQRS, flows, state machines, policies, aggregates,
value objects — live in [`backbone-core`](../backbone-core/) and know nothing
about HTTP, gRPC, or GraphQL. Transports are adapters bolted onto the core, not
the other way around. The same `CrudService` implementation serves an Axum HTTP
router and a gRPC service without change.

*Why it matters to you:* your business logic never imports a web framework. You
can change transports, or expose two at once, without touching the domain.

### 3. Pluggable backends

Every infrastructure crate ships **multiple implementations behind a trait**,
and you pick one with a feature flag or config — never a code change:

- cache → Memory · Redis
- storage → S3 · MinIO · Local
- email → SMTP · SES · Mailgun
- queue → Redis · RabbitMQ · SQS
- search → Elasticsearch · Algolia

Development runs on the in-memory / local backend; production swaps in the real
one. Your code depends on the trait, so the swap is a config line.

### 4. Consistency by generation, not by discipline

An entity built on `backbone-core` automatically exposes the same **standard
CRUD surface** — the "11 Backbone endpoints" plus the atomic-batch and trash
operations — identically across HTTP and gRPC. You do not hand-write list
handlers, pagination, filtering, soft-delete, or bulk operations for the
hundredth time. Consistency across a hundred entities is a property of the
framework, not something a hundred developers have to remember to uphold.

## What Backbone deliberately is *not* (non-goals)

Trust comes from honest limits. Backbone does **not**:

- **Distribute on crates.io.** It is consumed as **git dependencies pinned to a
  release tag**. This is a conscious choice ([ADR-0001](adr/adr-0001-git-tag-distribution.md)),
  not an oversight, and it shapes how you depend on it.
- **Own your `main.rs`.** It is a set of libraries (`lib.rs`, no `main.rs` in the
  framework crates). You compose it into *your* service; it does not scaffold one
  for you. Scaffolding is the job of the `metaphor` CLI and the `module` projects,
  which live outside this workspace.
- **Hold business/domain logic.** The framework crates are **plumbing**. Domain
  logic belongs in the `module` projects of the wider Metaphor workspace. A crate
  that grew domain rules would be a bug in the layering.
- **Guarantee semver per crate.** The whole workspace shares **one version**
  ([ADR-0004](adr/adr-0004-monorepo-versioning.md)); a major bump means *something*
  in the workspace broke, not that the crate you use did.
- **Pin you to one database or transport.** Where a trait-based abstraction is
  viable, the core stays backend-agnostic and adapters sit behind features.

## Who Backbone is for

Teams building **multiple** Rust services (or one service with many entities)
who are tired of re-deriving the same CRUD-and-plumbing layer and want it to be
identical everywhere — while keeping the freedom to extract, replace, or walk
away from any one piece.

If you are building a single small service and want a batteries-included
`main.rs` that runs on `cargo run`, a heavier opinionated framework will get you
there faster. Backbone optimizes for the *fleet*, not the one-off.

## Where to go next

- The **[Background](background.md)** page places Backbone against the tools that
  came before it.
- The **[Technology](technology.md)** page gives the reasoning behind each
  library and runtime choice.
- Ready to build? Jump to the **[Developer guide](developer-guide.md)**.
