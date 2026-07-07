<!-- Reader: All · Mode: Navigation -->
# Backbone Framework — Handbook

A modular Rust framework for building production-grade backend services. This
handbook is the top-level map of the project. Every page below names the one
reader it is written for — start with the row that matches you.

> Version at time of writing: **2.6.1** (see [`Cargo.toml`](../Cargo.toml),
> `[workspace.metadata.release].version`). Where behavior is version-specific,
> the page says so.

## Start here by who you are

| You are… | You want… | Read |
|----------|-----------|------|
| **Evaluating** Backbone | Why it exists, whether to adopt it | [Philosophy](philosophy.md) → [Background & prior art](background.md) → [Technology & the "why"](technology.md) |
| **Building a service** on it | Install, quickstart, recipes | [Developer guide](developer-guide.md) |
| **Maintaining** the framework | How it works, how to extend it safely | [Architecture](architecture.md) → [Maintainer guide](maintainer-guide.md) |
| **Contributing** a change | Setup, conventions, PR flow | [Contribution guide](contributing.md) |
| **Anyone** | One term, one meaning | [Glossary](glossary.md) |

## The whole handbook

1. **[Philosophy & motivation](philosophy.md)** — the problem, the worldview, the non-goals.
2. **[Background & prior art](background.md)** — what came before and what Backbone borrows or rejects.
3. **[Technology & the "why"](technology.md)** — the stack, each choice with a rationale and a rejected alternative.
4. **[Architecture](architecture.md)** — C4 context → containers → crates → one request traced end-to-end.
5. **[Maintainer guide](maintainer-guide.md)** — add a crate, add a backend, cut a release, without breaking conventions.
6. **[Developer guide](developer-guide.md)** — install → quickstart → key concepts → recipes → configuration → troubleshooting.
7. **[Contribution guide](contributing.md)** — dev setup, commit/PR conventions, review expectations.
8. **[Glossary](glossary.md)** — the ubiquitous language used everywhere in this handbook.
9. **[Architecture Decision Records](adr/README.md)** — one decision per record, why this design and not another.

## Per-crate documentation

This handbook is the framework-altitude view. Each member crate also ships its
own `README.md` and, in the case of [`backbone-core`](../backbone-core/docs/README.md),
a full `docs/` set (architecture, usage, API reference, configuration, OpenAPI).
When this handbook and a crate's own docs disagree, the crate's docs and the
code win — please [flag the drift](contributing.md).
