---
id: 01M0XN1DGZ4J25H8D9PJK8JKSZ
number: 1
title: "Wave 4 platform pass: supply-chain module audits, pin probe, register-delta floor, closing council"
type: story
status: done
reporter: faridlab
created: 2026-08-25T23:45:40Z
updated: 2026-08-25T23:45:40Z
---

Read-only 10-agent fleet + council over backbone-inventory, backbone-buying, backbone-manufacturing, backbone-quality and the serpa-service host seams.

Findings (chair-verified): only inventory pin-aligned (framework crates tag-equal v2.7.9); buying/manufacturing float branch=main, quality also carries a stale normal-edge outbox pin v2.7.4; accounting split (app v0.6.5 vs modules v0.4.0); inventory committed lock fails cargo metadata --locked (exit 101); buying three-way lock disagreement + 2 untagged commits; company_fence declarations absent module-wide in all four; undeclared hand-written files: inventory 11, buying 1 pair, manufacturing 6, quality 7 (incl. relay-critical outbox DDL); zero crons in all four; ADR-0015 enforcement keys absent (pilot-era) in buying/manufacturing.

Register-delta floor: 48 stock + 60 purchase + 39 mrp + 4 quality rows at docs/plan/w4-register-deltas.md (re-audited by ID at each family pass). Corrected residue map: stock T9, stock_account V1/V2 V7-V10 V15/V16 VT2-VT10, LC1-LC18, purchase T9; MEX-7 removed (does not exist).

Owner calls 2026-08-26: P1 runs now, re-pin train (inventory v0.4.4 -> buying v0.3.4 -> manufacturing v0.3.2 -> quality v0.5.3, step 0 lock hygiene, path deps to tag pins, per-tag --locked gate) lands before P3; S-7 barcodes closed (no engine in W4, barcodes rows to W7); framework v2.7.10 cut over 3b90808 as hygiene.

Pre-P1 landings: inventory user_owned block (11 files) before first --force regen; lock quarantine + --locked-passing regen + accounting dev-dep v0.6.5 + fence declarations + user_owned folded into one convergence tag.

Council record: docs/council/2026-08-26-module-w4-p0-platform-pass.md
