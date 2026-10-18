//! The tables a workflow engine keeps, as SQL.
//!
//! The schema generator writes the same statements into a module's migration; this function is the
//! framework's own copy, for tests and for a service that creates the tables itself.

/// The statuses an instance moves through.
pub const STATUSES: &[&str] = &[
    "running",
    "waiting",
    "completed",
    "failed",
    "expired",
    "compensating",
    "compensated",
    "cancelled",
];

/// Whether `schema` is a plain lowercase identifier, safe to put in a statement.
pub fn valid_schema(schema: &str) -> bool {
    let mut chars = schema.chars();
    matches!(chars.next(), Some(c) if c.is_ascii_lowercase() || c == '_')
        && chars.all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_')
        && schema.len() <= 48
}

/// The statements that create `bb_workflow_instances` and `bb_workflow_steps` in `schema`, fence
/// them by `company_id` with row-level security. The outbox and inbox tables the engine stages events
/// in and dedups resumes with are `backbone_core::write_hooks::migrate_outbox`'s (a module that stages
/// events runs it already). Run it as the table owner. Not idempotent for the policies' sake: run it
/// once, as a migration does; [`drop_and_create_policies`] is for a test that re-creates them.
///
/// # Panics
/// On a `schema` that is not a plain lowercase identifier.
pub fn migration_sql(schema: &str) -> String {
    assert!(
        valid_schema(schema),
        "`{schema}` is not a plain schema name"
    );
    let statuses = STATUSES
        .iter()
        .map(|s| format!("'{s}'"))
        .collect::<Vec<_>>()
        .join(", ");
    format!(
        r#"CREATE SCHEMA IF NOT EXISTS {schema};

CREATE TABLE IF NOT EXISTS {schema}.bb_workflow_instances (
    id               uuid PRIMARY KEY,
    workflow         text NOT NULL,
    version          int  NOT NULL,
    company_id       uuid NOT NULL,
    org_unit_id      uuid,
    status           text NOT NULL CHECK (status IN ({statuses})),
    current_step     text,
    context          jsonb NOT NULL DEFAULT '{{}}'::jsonb,
    trigger_event_id uuid NOT NULL,
    correlation_id   text,
    deadline_at      timestamptz,
    wait_event       text,
    wait_key         text,
    wait_due_at      timestamptz,
    attempts         int  NOT NULL DEFAULT 0,
    last_error       text,
    created_at       timestamptz NOT NULL DEFAULT now(),
    updated_at       timestamptz NOT NULL DEFAULT now(),
    CONSTRAINT uq_bb_workflow_instances_trigger UNIQUE (workflow, trigger_event_id)
);

CREATE INDEX IF NOT EXISTS idx_bb_workflow_instances_due
    ON {schema}.bb_workflow_instances (wait_due_at)
    WHERE status IN ('waiting', 'running') AND wait_due_at IS NOT NULL;
CREATE INDEX IF NOT EXISTS idx_bb_workflow_instances_wait
    ON {schema}.bb_workflow_instances (wait_event, wait_key)
    WHERE status = 'waiting';
CREATE INDEX IF NOT EXISTS idx_bb_workflow_instances_unfinished
    ON {schema}.bb_workflow_instances (status, updated_at)
    WHERE status IN ('running', 'compensating');

CREATE TABLE IF NOT EXISTS {schema}.bb_workflow_steps (
    instance_id uuid NOT NULL REFERENCES {schema}.bb_workflow_instances (id) ON DELETE CASCADE,
    company_id  uuid NOT NULL,
    org_unit_id uuid,
    seq         int  NOT NULL,
    step        text NOT NULL,
    outcome     text NOT NULL,
    error       text,
    started_at  timestamptz NOT NULL DEFAULT now(),
    finished_at timestamptz NOT NULL DEFAULT now(),
    PRIMARY KEY (instance_id, seq)
);

ALTER TABLE {schema}.bb_workflow_instances ENABLE ROW LEVEL SECURITY;
ALTER TABLE {schema}.bb_workflow_instances FORCE ROW LEVEL SECURITY;
ALTER TABLE {schema}.bb_workflow_steps ENABLE ROW LEVEL SECURITY;
ALTER TABLE {schema}.bb_workflow_steps FORCE ROW LEVEL SECURITY;
{policies}"#,
        policies = policies_sql(schema)
    )
}

fn policies_sql(schema: &str) -> String {
    // The sweep reads across companies to find what is due, with `app.workflow_sweep` set for that
    // one read; it can only read: the check clause stays the company, so nothing is written
    // outside a company's own scope.
    format!(
        r#"CREATE POLICY bb_workflow_instances_company_isolation ON {schema}.bb_workflow_instances FOR ALL
    USING      (company_id = NULLIF(current_setting('app.company_id', true), '')::uuid
                OR current_setting('app.workflow_sweep', true) = 'on')
    WITH CHECK (company_id = NULLIF(current_setting('app.company_id', true), '')::uuid);
CREATE POLICY bb_workflow_steps_company_isolation ON {schema}.bb_workflow_steps FOR ALL
    USING      (company_id = NULLIF(current_setting('app.company_id', true), '')::uuid)
    WITH CHECK (company_id = NULLIF(current_setting('app.company_id', true), '')::uuid);
"#
    )
}

/// The statements that fence both tables by org unit instead of by company (ADR-0029), for a module
/// whose tables are org-scoped: the `<table>_org_unit_isolation` policy (the entitlement union, plus
/// the read-only sweep clause) replaces the company policy, and a trigger fills a NULL `org_unit_id`
/// from the acting unit. A row with no org unit is then refused loudly. The generated migration
/// adds the kind guard over `organization.org_units` to this.
///
/// # Panics
/// On a `schema` that is not a plain lowercase identifier.
pub fn org_fence_sql(schema: &str) -> String {
    assert!(
        valid_schema(schema),
        "`{schema}` is not a plain schema name"
    );
    let mut out = String::new();
    for table in ["bb_workflow_instances", "bb_workflow_steps"] {
        let sweep = if table == "bb_workflow_instances" {
            "\n                OR current_setting('app.workflow_sweep', true) = 'on'"
        } else {
            ""
        };
        out.push_str(&format!(
            r#"DROP POLICY IF EXISTS {table}_company_isolation ON {schema}.{table};
DROP POLICY IF EXISTS {table}_org_unit_isolation ON {schema}.{table};
CREATE POLICY {table}_org_unit_isolation ON {schema}.{table} FOR ALL
    USING      (org_unit_id = ANY(string_to_array(current_setting('app.scope_unit_ids', true), ',')::uuid[]){sweep})
    WITH CHECK (org_unit_id = ANY(string_to_array(current_setting('app.scope_unit_ids', true), ',')::uuid[]));
CREATE OR REPLACE FUNCTION {schema}.{table}_org_unit_fill() RETURNS trigger AS $$
BEGIN
    IF NEW.org_unit_id IS NULL THEN
        NEW.org_unit_id := nullif(current_setting('app.acting_unit_id', true), '')::uuid;
    END IF;
    RETURN NEW;
END;
$$ LANGUAGE plpgsql;
DROP TRIGGER IF EXISTS {table}_org_unit_fill ON {schema}.{table};
CREATE TRIGGER {table}_org_unit_fill BEFORE INSERT ON {schema}.{table}
    FOR EACH ROW EXECUTE FUNCTION {schema}.{table}_org_unit_fill();
"#
        ));
    }
    out
}

/// The statements that replace both policies, for a test that creates the tables more than once.
pub fn drop_and_create_policies(schema: &str) -> String {
    assert!(
        valid_schema(schema),
        "`{schema}` is not a plain schema name"
    );
    format!(
        "DROP POLICY IF EXISTS bb_workflow_instances_company_isolation ON {schema}.bb_workflow_instances;\n\
         DROP POLICY IF EXISTS bb_workflow_steps_company_isolation ON {schema}.bb_workflow_steps;\n{}",
        policies_sql(schema)
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_migration_names_the_tables_the_fence_and_the_dedup_table() {
        let sql = migration_sql("hr");
        for needle in [
            "hr.bb_workflow_instances",
            "hr.bb_workflow_steps",
            "UNIQUE (workflow, trigger_event_id)",
            "FORCE ROW LEVEL SECURITY",
            "'compensating'",
        ] {
            assert!(sql.contains(needle), "{needle}");
        }
    }

    #[test]
    fn a_schema_that_is_not_a_plain_identifier_is_refused() {
        assert!(valid_schema("selling_2"));
        for bad in ["", "Hr", "hr; drop", "9x", "a-b"] {
            assert!(!valid_schema(bad), "{bad}");
        }
    }
}
