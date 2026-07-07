# backbone-outbox

Durable **transactional-outbox + relay + inbox** primitives for go-live exactly-once event delivery over
Postgres — without a broker. Composes the in-process `backbone-messaging` bus; does not replace it.

- **`outbox::stage(&mut tx, record)`** — write a serialized event into `<schema>.outbox_events` **in the
  producer's own transaction**, so the state change and the event commit atomically (no lost/phantom
  events). Idempotent on the event id.
- **`relay::drain_once(pool, schema, batch, publish)`** — drain un-published rows (`FOR UPDATE SKIP
  LOCKED`) onto a caller-supplied `publish` sink and mark them; **at-least-once**. Host it on a
  `backbone-jobs` schedule; wire `publish` to the in-proc bus today, a broker later.
- **`inbox::once(&mut tx, schema, consumer, event_id)`** — dedup a `(consumer, event_id)` in the
  consumer's transaction, turning at-least-once delivery into an **exactly-once effect**.

Framework plumbing: depends on `sqlx` + `serde` only — never on a domain module or the bus. See
`docs/erp/event-bus-contract.md` for the full contract and the rollout checklist.

```rust
outbox::migrate(&pool, "payment").await?;                 // create outbox_events + inbox_consumed
// producer:
let mut tx = pool.begin().await?;
// ... mutate state ...
outbox::stage(&mut *tx, "payment", &record).await?;
tx.commit().await?;
// relay:
relay::drain_once(&pool, "payment", 100, |rec| async move { bus.publish(rec).await }).await?;
// consumer:
let mut tx = pool.begin().await?;
if inbox::once(&mut *tx, "billing", "settlement-consumer", event_id).await? { /* apply */ }
tx.commit().await?;
```
