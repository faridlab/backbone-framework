//! The producer side: schema setup + staging an event **inside the producer's transaction**.

use sqlx::{PgExecutor, PgPool};

use crate::error::{validate_schema, OutboxError, Result};
use crate::record::OutboxRecord;

/// Create the `outbox_events` + `inbox_consumed` tables in `schema` (idempotent). A module is both a
/// producer (its outbox) and a consumer (its inbox), so both are created together. Safe to run on every
/// boot.
/// Build an index name the way Postgres will store it.
///
/// Identifiers are truncated to 63 bytes on creation. Constructing the same name twice — once to
/// create and once to drop — only works if both are truncated identically: a long schema name would
/// otherwise be created truncated and dropped by a name that matches nothing, silently leaving the
/// index behind. Schema names are validated as ASCII (`^[a-z_][a-z0-9_]*$`), so byte truncation
/// cannot split a character.
fn index_name(schema: &str, suffix: &str) -> String {
    const MAX_IDENTIFIER_BYTES: usize = 63;
    let mut name = format!("idx_{schema}_{suffix}");
    name.truncate(MAX_IDENTIFIER_BYTES);
    name
}

pub async fn migrate(pool: &PgPool, schema: &str) -> Result<()> {
    validate_schema(schema)?;
    sqlx::query(&format!("CREATE SCHEMA IF NOT EXISTS {schema}")).execute(pool).await?;
    sqlx::query(&format!(
        r#"CREATE TABLE IF NOT EXISTS {schema}.outbox_events (
             id             uuid PRIMARY KEY,
             event_type     text NOT NULL,
             aggregate_type text NOT NULL,
             aggregate_id   text NOT NULL,
             company_id     uuid NOT NULL,
             payload        jsonb NOT NULL,
             occurred_at    timestamptz NOT NULL,
             correlation_id text,
             causation_id   text,
             version        int NOT NULL DEFAULT 1,
             created_at     timestamptz NOT NULL DEFAULT now(),
             published_at   timestamptz
           )"#
    ))
    .execute(pool)
    .await?;
    // Heal a table created by an older version of this function that predates the `company_id`
    // column: `CREATE TABLE IF NOT EXISTS` is a no-op on such a table, so the fence/index/policy DDL
    // below would abort on the missing column and kill the boot. Added NULLABLE — a NOT NULL add
    // is rejected outright on any legacy table that already has rows. Rows that predate the heal
    // keep `company_id IS NULL`: the fence below hides them from every scoped app role (fail-closed),
    // while the relay role's policy bypass still sees them.
    sqlx::query(&format!(
        "ALTER TABLE {schema}.outbox_events ADD COLUMN IF NOT EXISTS company_id uuid"
    ))
    .execute(pool)
    .await?;
    // A record the transport has given up on is marked dead here rather than published: not
    // delivered, still visible, and re-emittable. Added the same way as `company_id` above — nullable
    // and IF NOT EXISTS — so a table created by an older version of this function heals in place.
    sqlx::query(&format!(
        "ALTER TABLE {schema}.outbox_events ADD COLUMN IF NOT EXISTS failed_at timestamptz"
    ))
    .execute(pool)
    .await?;
    sqlx::query(&format!(
        "ALTER TABLE {schema}.outbox_events ADD COLUMN IF NOT EXISTS failure_reason text"
    ))
    .execute(pool)
    .await?;

    // ADR-0029's org axis. Installed unconditionally, the same way `company_id` is: the column
    // belongs with the record, only the RLS fence below is opt-in. Nullable on purpose — a table
    // created by an older version of this function heals in place, and a deployment with no org
    // spine simply leaves it NULL.
    sqlx::query(&format!(
        "ALTER TABLE {schema}.outbox_events ADD COLUMN IF NOT EXISTS org_unit_id uuid"
    ))
    .execute(pool)
    .await?;
    // Carry existing rows over. The org spine copied company ids verbatim when it was built, so the
    // two identifiers are the same value for any row staged before the re-key — the same identity
    // the tenancy decorator relies on when it backfills a module's table.
    sqlx::query(&format!(
        "UPDATE {schema}.outbox_events SET org_unit_id = company_id
           WHERE org_unit_id IS NULL AND company_id IS NOT NULL"
    ))
    .execute(pool)
    .await?;
    // Stamp the acting unit on a row that arrives without one. `stage` does not name the column, so
    // this is what fills it; a writer that names every column and sends an explicit NULL is covered
    // too, since the trigger tests the value rather than its absence. An unbound scope leaves NULL,
    // which the fence below then refuses — fail-closed, not fail-quiet.
    sqlx::query(&format!(
        r#"CREATE OR REPLACE FUNCTION {schema}.outbox_events_org_unit_fill() RETURNS trigger AS $$
           BEGIN
               IF NEW.org_unit_id IS NULL THEN
                   NEW.org_unit_id := NULLIF(current_setting('app.acting_unit_id', true), '')::uuid;
               END IF;
               RETURN NEW;
           END;
           $$ LANGUAGE plpgsql"#
    ))
    .execute(pool)
    .await?;
    sqlx::query(&format!(
        "DROP TRIGGER IF EXISTS outbox_events_org_unit_fill ON {schema}.outbox_events"
    ))
    .execute(pool)
    .await?;
    sqlx::query(&format!(
        "CREATE TRIGGER outbox_events_org_unit_fill
           BEFORE INSERT ON {schema}.outbox_events
           FOR EACH ROW EXECUTE FUNCTION {schema}.outbox_events_org_unit_fill()"
    ))
    .execute(pool)
    .await?;

    // Partial index over just the un-drained tail — keeps the relay's poll cheap as the table grows.
    // Dead rows are excluded because the relay no longer offers them to the transport.
    let pending_idx = index_name(schema, "outbox_pending");
    sqlx::query(&format!(
        "CREATE INDEX IF NOT EXISTS {pending_idx}
           ON {schema}.outbox_events (occurred_at) WHERE published_at IS NULL AND failed_at IS NULL"
    ))
    .execute(pool)
    .await?;
    // Retire the predecessor, whose predicate still admitted dead rows. Dropped rather than left in
    // place so the tail is covered by exactly one partial index; `IF EXISTS` makes the boot after the
    // first one a no-op.
    let legacy_idx = index_name(schema, "outbox_unpublished");
    if legacy_idx == pending_idx {
        // Truncation ate both suffixes, so the drop below would remove the index just created and
        // the create above would have been skipped over a stale one. No real schema name is this
        // long; refuse rather than quietly leave the drain's tail wrongly indexed.
        return Err(OutboxError::InvalidSchema(format!(
            "{schema}: too long to build distinct index names within Postgres's 63-byte limit"
        )));
    }
    sqlx::query(&format!("DROP INDEX IF EXISTS {schema}.{legacy_idx}"))
        .execute(pool)
        .await?;

    // ADR-0029: fence `outbox_events` by `org_unit_id` so a tenant's event stream is isolated. This is
    // the table OWNER applying the fence (the correct home — it was previously a hand-authored backfill
    // migration bolted onto each module). Opt-in via the `multi_tenant` feature so the framework stays
    // tenant-agnostic; a company-tenant service enables it. The `company_id` column is guaranteed
    // present above — created with the table, or healed in place by the ALTER for a legacy table —
    // so only the RLS fence is conditional. An unset `app.company_id`
    // session var sees zero rows (NULLIF → NULL), the standard fail-closed posture. The outbox relay is
    // cross-tenant (it drains `WHERE published_at IS NULL` with no company scope), so the policy also
    // admits connections logged in as `metaphor_relay` (`OR current_user = 'metaphor_relay'`) — a surgical
    // per-table bypass, NOT a BYPASSRLS attribute, so every other table's fence still holds.
    #[cfg(feature = "multi_tenant")]
    {
        let org_idx = index_name(schema, "outbox_org_unit_id");
        sqlx::query(&format!(
            "CREATE INDEX IF NOT EXISTS {org_idx} ON {schema}.outbox_events (org_unit_id)"
        ))
        .execute(pool)
        .await?;
        sqlx::query(&format!("ALTER TABLE {schema}.outbox_events ENABLE ROW LEVEL SECURITY"))
            .execute(pool).await?;
        sqlx::query(&format!("ALTER TABLE {schema}.outbox_events FORCE ROW LEVEL SECURITY"))
            .execute(pool).await?;
        // The org fence, in the shape the tenancy decorator installs everywhere else: membership of
        // the session's entitlement union, not equality with a single id. A session may be entitled
        // to a subtree, and a row staged on any unit of it is the session's to see.
        //
        // The whole swap goes in ONE simple query, which Postgres runs in a single implicit
        // transaction. That is what makes it both repeatable and safe. `CREATE POLICY` has no
        // IF NOT EXISTS, so a second boot needs the drop in front of it; but a table under FORCE RLS
        // with no policy denies everything, so a drop that is visible on its own would stall the
        // relay mid-drain. Inside one transaction no other session ever observes the gap.
        sqlx::raw_sql(&format!(
            r#"DROP POLICY IF EXISTS outbox_events_org_unit_isolation ON {schema}.outbox_events;
               CREATE POLICY outbox_events_org_unit_isolation ON {schema}.outbox_events
                 FOR ALL
                 USING      (org_unit_id = ANY(string_to_array(
                                 current_setting('app.scope_unit_ids', true), ',')::uuid[])
                             OR current_user = 'metaphor_relay')
                 WITH CHECK (org_unit_id = ANY(string_to_array(
                                 current_setting('app.scope_unit_ids', true), ',')::uuid[])
                             OR current_user = 'metaphor_relay');
               DROP POLICY IF EXISTS outbox_events_company_isolation ON {schema}.outbox_events;"#
        ))
        .execute(pool)
        .await?;
        // The company index goes with the policy that used it. The column itself stays until every
        // producer stops filling it.
        let company_idx = index_name(schema, "outbox_company_id");
        sqlx::query(&format!("DROP INDEX IF EXISTS {schema}.{company_idx}"))
            .execute(pool)
            .await?;
    }

    sqlx::query(&format!(
        r#"CREATE TABLE IF NOT EXISTS {schema}.inbox_consumed (
             consumer    text NOT NULL,
             event_id    uuid NOT NULL,
             consumed_at timestamptz NOT NULL DEFAULT now(),
             PRIMARY KEY (consumer, event_id)
           )"#
    ))
    .execute(pool)
    .await?;
    Ok(())
}

/// Stage an event into `schema.outbox_events`. Pass the producer's **transaction** as the executor
/// (`&mut *tx`) so the event and the state change commit atomically — no lost or phantom events.
///
/// Idempotent on the event `id` (`ON CONFLICT DO NOTHING`): a producer-level retry re-stages harmlessly.
/// Returns `true` if the row was newly staged, `false` if this id was already present.
pub async fn stage<'c, E>(executor: E, schema: &str, rec: &OutboxRecord) -> Result<bool>
where
    E: PgExecutor<'c>,
{
    validate_schema(schema)?;
    let done = sqlx::query(&format!(
        r#"INSERT INTO {schema}.outbox_events
             (id, event_type, aggregate_type, aggregate_id, company_id, payload, occurred_at,
              correlation_id, causation_id, version)
           VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10)
           ON CONFLICT (id) DO NOTHING"#
    ))
    .bind(rec.id)
    .bind(&rec.event_type)
    .bind(&rec.aggregate_type)
    .bind(&rec.aggregate_id)
    .bind(rec.company_id)
    .bind(&rec.payload)
    .bind(rec.occurred_at)
    .bind(&rec.correlation_id)
    .bind(&rec.causation_id)
    .bind(rec.version)
    .execute(executor)
    .await?;
    Ok(done.rows_affected() == 1)
}

/// Count of events the transport gave up on — dead-lettered, awaiting a re-emit.
///
/// Worth alerting on: it is the number of cross-module effects that did not happen. A silently
/// published row used to hide exactly this figure.
pub async fn dead_count(pool: &PgPool, schema: &str) -> Result<i64> {
    validate_schema(schema)?;
    let n: i64 = sqlx::query_scalar(&format!(
        "SELECT count(*) FROM {schema}.outbox_events WHERE failed_at IS NOT NULL"
    ))
    .fetch_one(pool)
    .await?;
    Ok(n)
}

/// Count of un-drained events in `schema.outbox_events` (for monitoring / tests).
pub async fn pending_count(pool: &PgPool, schema: &str) -> Result<i64> {
    validate_schema(schema)?;
    let n: i64 = sqlx::query_scalar(&format!(
        "SELECT count(*) FROM {schema}.outbox_events WHERE published_at IS NULL AND failed_at IS NULL"
    ))
    .fetch_one(pool)
    .await?;
    Ok(n)
}

#[cfg(test)]
mod tests {
    use super::index_name;

    #[test]
    fn a_short_schema_keeps_its_full_index_name() {
        assert_eq!(index_name("payment", "outbox_pending"), "idx_payment_outbox_pending");
    }

    #[test]
    fn a_long_schema_is_truncated_the_way_postgres_truncates() {
        // Long enough that "idx_<schema>_outbox_pending" passes the 63-byte identifier cap.
        let schema = "a".repeat(50);
        let name = index_name(&schema, "outbox_pending");
        assert_eq!(name.len(), 63, "the name is capped at the identifier limit");
        assert_eq!(name, index_name(&schema, "outbox_pending"), "create and drop name the same index");
    }

    #[test]
    fn a_schema_long_enough_to_collapse_both_suffixes_is_refused() {
        // At this length truncation eats the suffix entirely and both index names become the same
        // string, so dropping the predecessor would remove the index just created. `migrate` refuses
        // instead; this records where that line sits.
        let schema = "b".repeat(60);
        assert_eq!(
            index_name(&schema, "outbox_pending"),
            index_name(&schema, "outbox_unpublished"),
            "the two names collapse, which is what migrate refuses"
        );
    }

    #[test]
    fn realistic_schema_names_are_nowhere_near_the_limit() {
        for schema in ["payment", "billing", "messaging", "tax", "selling"] {
            assert!(index_name(schema, "outbox_unpublished").len() < 63);
            assert_ne!(
                index_name(schema, "outbox_pending"),
                index_name(schema, "outbox_unpublished")
            );
        }
    }
}
