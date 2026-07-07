<!-- Reader: Maintainer · Mode: Reference -->
# Architecture Decision Records

One decision per record: the context, the decision, the alternatives, and the
consequences we live with. ADRs are **immutable once Accepted** — to change a
decision, write a new ADR that supersedes the old one and update its Status line;
do not edit an accepted decision in place.

| # | Decision | Status |
|---|----------|--------|
| [0001](adr-0001-git-tag-distribution.md) | Distribute via git tags, not crates.io | Accepted |
| [0002](adr-0002-self-describing-crates.md) | Self-describing crates; no workspace dependency inheritance | Accepted |
| [0003](adr-0003-protocol-agnostic-core.md) | Protocol-agnostic core with pluggable backends | Accepted |
| [0004](adr-0004-monorepo-versioning.md) | One version for the whole workspace | Accepted |

New ADRs start from the template at
[`.claude/skills/framework-handbook/templates/adr-NNNN.md`](../../.claude/skills/framework-handbook/templates/adr-NNNN.md).
