//! Org-tree request scope for the entitlement-union RLS fence (ADR-0028).
//!
//! ADR-0028 replaces the single `company_id` scoping key with `org_unit_id`: one org tree per
//! tenant database (root / company / branch nodes), and a session sees the UNION of the subtrees
//! under every node it is entitled to — always including the root node, which owns tenant-wide
//! shared rows. The database half is the policy
//! `org_unit_id = ANY(string_to_array(current_setting('app.scope_unit_ids', true), ',')::uuid[])`;
//! this module is the application half that resolves a session's entitled ids and carries them
//! for the duration of a request.
//!
//! During the module-by-module re-key both fences are live at once: some tables still read
//! `app.company_id` (ADR-0008 equality fence), org-re-keyed tables read `app.scope_unit_ids`.
//! [`with_org_request_scope`] therefore sets BOTH session variables on one request-dedicated
//! connection — the legacy variable resolved from the acting node's company ancestry, so a
//! session acting at a branch keeps its company-fenced rows too. When the last company-fenced
//! table is re-keyed, the legacy bridge retires with the old fence.
//!
//! The scope binds the same `REQUEST_CONN` task-local as
//! [`with_request_scope`](crate::company_scope::with_request_scope): every scoped execute helper
//! in this crate — and therefore every generated repository — runs on that connection and
//! inherits the fence variables without a single call-site change.
//!
//! **The task-local is not the fence.** RLS is. Unscoped statements see the variables unset and
//! match zero rows — fail-closed, identical to the ADR-0008 contract.

use sqlx::PgPool;
use std::fmt;
use std::future::Future;
use std::sync::Arc;
use tokio::sync::Mutex;
use uuid::Uuid;

tokio::task_local! {
    /// The resolved org scope of the current request: entitled unit ids (union of subtrees,
    /// root included) and the acting node. Unset for platform callers and non-request code.
    static ORG_SCOPE: Arc<OrgScope>;
}

/// A resolved session scope over the org tree (ADR-0028).
///
/// Built by [`resolve_org_scope`]; carried by [`with_org_request_scope`].
#[derive(Debug, Clone)]
pub struct OrgScope {
    scope_unit_ids: Vec<Uuid>,
    acting_unit_id: Uuid,
    /// The company node governing the acting node (itself for a company, nearest company
    /// ancestor for a branch). `None` when the chain has no company node — then no legacy
    /// `app.company_id` is set and company-fenced tables fail closed for this session.
    legacy_company_id: Option<Uuid>,
}

impl OrgScope {
    /// Every unit id this session may see rows for: the union of the entitled subtrees plus the
    /// root node. Order is stable (sorted) so the serialized form is deterministic.
    pub fn scope_unit_ids(&self) -> &[Uuid] {
        &self.scope_unit_ids
    }

    /// The node the session acts at — the default `org_unit_id` for new records and the UI's
    /// context node.
    pub fn acting_unit_id(&self) -> Uuid {
        self.acting_unit_id
    }

    /// The company node for the legacy `app.company_id` fence during the re-key transition.
    pub fn legacy_company_id(&self) -> Option<Uuid> {
        self.legacy_company_id
    }

    /// The scope ids in the exact wire form the fence policy parses: comma-joined, no spaces.
    fn scope_unit_ids_csv(&self) -> String {
        self.scope_unit_ids
            .iter()
            .map(|id| id.to_string())
            .collect::<Vec<_>>()
            .join(",")
    }
}

/// Failures of [`resolve_org_scope`] — all of them mean "do NOT open a scoped session".
#[derive(Debug)]
pub enum OrgScopeError {
    /// The acting node does not exist in `organization.org_units` (wrong id, or the org spine
    /// migration has not run in this database).
    UnknownActingUnit(Uuid),
    /// The database lacks the spine helpers (`organization.org_unit_subtree` / `org_unit_root`).
    MissingSpineHelpers,
    Database(sqlx::Error),
}

impl fmt::Display for OrgScopeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnknownActingUnit(id) => {
                write!(f, "org scope: acting unit {id} is not an organization.org_units node")
            }
            Self::MissingSpineHelpers => write!(
                f,
                "org scope: organization.org_unit_subtree / org_unit_root are missing — \
                 has the org spine migration run in this database?"
            ),
            Self::Database(e) => write!(f, "org scope: resolution query failed: {e}"),
        }
    }
}

impl std::error::Error for OrgScopeError {}

impl From<sqlx::Error> for OrgScopeError {
    fn from(e: sqlx::Error) -> Self {
        Self::Database(e)
    }
}

/// Resolve a session's scope from the org tree.
///
/// `acting_unit` is the node the session acts at (from the signed token / tenant registry).
/// `additional_entitled` are further nodes the session holds entitlements for (e.g. a group
/// admin entitled to a sister company) — each contributes its whole subtree. The resolved scope
/// is the union of those subtrees plus the root node.
///
/// `org_units` is unfenced by design (it IS the scoping substrate), so this runs on any
/// connection of the tenant database regardless of fence state.
pub async fn resolve_org_scope(
    conn: &mut sqlx::PgConnection,
    acting_unit: Uuid,
    additional_entitled: &[Uuid],
) -> Result<OrgScope, OrgScopeError> {
    let mut roots = vec![acting_unit];
    roots.extend_from_slice(additional_entitled);

    let scope_unit_ids: Vec<Uuid> = sqlx::query_scalar(
        "SELECT u.id FROM organization.org_unit_subtree($1::uuid[]) AS u(id) \
         UNION SELECT organization.org_unit_root()",
    )
    .bind(&roots)
    .fetch_all(&mut *conn)
    .await
    .map_err(|e| {
        // 42883 undefined_function / 42P01 undefined_table both mean the spine is absent.
        match &e {
            sqlx::Error::Database(db) if db.code().as_deref() == Some("42883")
                || db.code().as_deref() == Some("42P01") =>
            {
                OrgScopeError::MissingSpineHelpers
            }
            _ => OrgScopeError::Database(e),
        }
    })?;

    if scope_unit_ids.is_empty() {
        return Err(OrgScopeError::MissingSpineHelpers);
    }
    if !scope_unit_ids.contains(&acting_unit) {
        // subtree() silently drops unknown roots; surface that instead of opening a
        // narrower-than-asked scope (a wrong-but-working session is worse than a loud error).
        return Err(OrgScopeError::UnknownActingUnit(acting_unit));
    }

    // Walk up from the acting node to the governing company node for the legacy fence.
    let legacy_company_id: Option<Uuid> = sqlx::query_scalar(
        "WITH RECURSIVE up AS ( \
            SELECT id, parent_id, kind FROM organization.org_units WHERE id = $1 \
            UNION ALL \
            SELECT o.id, o.parent_id, o.kind FROM organization.org_units o \
              JOIN up ON o.id = up.parent_id \
         ) SELECT up.id FROM up WHERE up.kind::text = 'company' ORDER BY up.id LIMIT 1",
    )
    .bind(acting_unit)
    .fetch_optional(&mut *conn)
    .await?;

    let mut sorted = scope_unit_ids;
    sorted.sort_unstable();
    sorted.dedup();

    Ok(OrgScope {
        scope_unit_ids: sorted,
        acting_unit_id: acting_unit,
        legacy_company_id,
    })
}

/// Run `f` with a request-dedicated connection carrying the session's org scope.
///
/// Sets `app.scope_unit_ids` (the entitlement-union fence) AND the legacy `app.company_id`
/// (equality fence, resolved from the acting node's company ancestry) at the session level, so
/// org-re-keyed and not-yet-re-keyed tables are both fenced correctly for the whole request —
/// including ID-only lookups, which ride the connection rather than the query text.
///
/// Mirrors [`with_request_scope`](crate::company_scope::with_request_scope)'s reset discipline:
/// both variables are cleared unconditionally before the connection returns to the pool, even
/// when a `REQUEST_CONN` clone outlives the scope — a clone that runs queries after the reset
/// does so unscoped (fail-closed), never with the previous session's scope.
pub async fn with_org_request_scope<F, R>(pool: &PgPool, scope: OrgScope, f: F) -> Result<R, sqlx::Error>
where
    F: Future<Output = R>,
{
    let mut conn = pool.acquire().await?;
    sqlx::query("SELECT set_config('app.scope_unit_ids', $1, false)")
        .bind(scope.scope_unit_ids_csv())
        .execute(&mut *conn)
        .await?;
    sqlx::query("SELECT set_config('app.company_id', $1, false)")
        .bind(scope.legacy_company_id.map(|id| id.to_string()).unwrap_or_default())
        .execute(&mut *conn)
        .await?;

    let holder = Arc::new(Mutex::new(conn));
    let scope_arc = Arc::new(scope);
    let result = ORG_SCOPE.scope(scope_arc.clone(), async {
        crate::company_scope::with_company_scope_internal(scope_arc.legacy_company_id, async {
            crate::company_scope::with_request_conn_internal(holder.clone(), f).await
        })
        .await
    })
    .await;

    // Unconditional reset of BOTH fence variables (see with_request_scope for the
    // lingering-clone reasoning — the same contract applies to both variables).
    {
        let mut guard = holder.lock().await;
        for var in ["app.scope_unit_ids", "app.company_id"] {
            if let Err(e) = sqlx::query("SELECT set_config($1, '', false)")
                .bind(var)
                .execute(&mut **guard)
                .await
            {
                tracing::error!(
                    target: "backbone_orm::org_scope",
                    error = %e,
                    var,
                    "failed to reset fence variable on org request connection; the pool \
                     connection may carry the previous session's scope — treat as a \
                     fence-hygiene incident",
                );
            }
        }
    }
    Ok(result)
}

/// The resolved org scope of the current request, if one is bound.
///
/// Services use the acting node as the default `org_unit_id` for new records; `None` means no
/// org scope (platform caller or non-request code) — do not guess a node then.
pub fn current_org_scope() -> Option<OrgScope> {
    ORG_SCOPE.try_with(|s| s.as_ref().clone()).ok()
}

/// Bind an explicit org scope onto an already-open transaction/connection, transaction-locally.
///
/// The transaction twin of [`with_org_request_scope`], for hand-written write services and jobs
/// that manage their own transaction (mirrors
/// [`bind_company_on`](crate::company_scope::bind_company_on)).
pub async fn bind_org_scope_on(
    conn: &mut sqlx::PgConnection,
    scope: &OrgScope,
) -> Result<(), sqlx::Error> {
    sqlx::query("SELECT set_config('app.scope_unit_ids', $1, true)")
        .bind(scope.scope_unit_ids_csv())
        .execute(&mut *conn)
        .await?;
    sqlx::query("SELECT set_config('app.company_id', $1, true)")
        .bind(scope.legacy_company_id.map(|id| id.to_string()).unwrap_or_default())
        .execute(&mut *conn)
        .await?;
    Ok(())
}

/// Statement-level org fence for a write whose row belongs to one org node — the hand-written
/// repository path for callers outside a request scope.
///
/// Inside a request scope ([`with_org_request_scope`] or the legacy
/// [`with_request_scope`](crate::company_scope::with_request_scope)) the query runs on the
/// request connection, which already carries both fence variables. Outside one, it opens a
/// short transaction and sets `app.scope_unit_ids` to exactly `unit`: the INSERT's
/// `WITH CHECK` only tests the row's own `org_unit_id`, so the row's node alone satisfies it —
/// and a read that accidentally rides this helper sees only that node (fail-narrow, never
/// leaky). Reads that need the session's whole entitlement union must run inside a resolved
/// request scope, not this helper.
pub async fn execute_unit_scoped<'q>(
    pool: &PgPool,
    unit: Uuid,
    query: sqlx::query::Query<'q, sqlx::Postgres, sqlx::postgres::PgArguments>,
) -> Result<sqlx::postgres::PgQueryResult, sqlx::Error> {
    if let Some(conn) = crate::company_scope::current_request_conn() {
        let mut g = conn.lock().await;
        return query.execute(&mut **g).await;
    }
    let mut tx = pool.begin().await?;
    sqlx::query("SELECT set_config('app.scope_unit_ids', $1, true)")
        .bind(unit.to_string())
        .execute(&mut *tx)
        .await?;
    let res = query.execute(&mut *tx).await?;
    tx.commit().await?;
    Ok(res)
}

#[cfg(test)]
mod tests {
    //! Gated on `BACKBONE_ORM_RLS_DSN` (a superuser DSN). Self-contained: builds a minimal org
    //! spine + an org-fenced table + an app role, then proves the resolver and the request
    //! scope against them — the same shapes the organization/inventory migrations emit.
    use super::{resolve_org_scope, with_org_request_scope};
    use sqlx::postgres::PgPoolOptions;
    use sqlx::PgPool;
    use uuid::Uuid;

    fn dsn() -> Option<String> {
        std::env::var("BACKBONE_ORM_RLS_DSN").ok()
    }

    async fn admin_pool(dsn: &str) -> PgPool {
        PgPoolOptions::new().max_connections(4).connect(dsn).await.unwrap()
    }

    async fn app_pool(dsn: &str) -> PgPool {
        let after_at = dsn.rsplit('@').next().unwrap();
        let url = format!("postgresql://org_scope_app:orgpw@{after_at}");
        PgPoolOptions::new().max_connections(1).connect(&url).await.unwrap()
    }

    async fn setup(admin: &PgPool, root: Uuid, company: Uuid, branch: Uuid, other: Uuid) {
        sqlx::raw_sql(
            "DROP SCHEMA IF EXISTS organization CASCADE; \
             DROP SCHEMA IF EXISTS org_scope_test CASCADE; \
             DROP ROLE IF EXISTS org_scope_app; \
             CREATE SCHEMA organization; \
             CREATE TABLE organization.org_units ( \
                 id uuid PRIMARY KEY, kind text NOT NULL, parent_id uuid, \
                 code text, name text NOT NULL, metadata jsonb NOT NULL DEFAULT '{}' \
             ); \
             CREATE UNIQUE INDEX one_root ON organization.org_units (kind) WHERE kind = 'root'; \
             CREATE OR REPLACE FUNCTION organization.org_unit_subtree(p_roots uuid[]) \
                 RETURNS SETOF uuid LANGUAGE sql STABLE AS $$ \
                 WITH RECURSIVE tree AS ( \
                     SELECT o.id FROM organization.org_units o WHERE o.id = ANY(p_roots) \
                     UNION ALL \
                     SELECT o.id FROM organization.org_units o JOIN tree t ON o.parent_id = t.id \
                 ) SELECT id FROM tree $$; \
             CREATE OR REPLACE FUNCTION organization.org_unit_root() RETURNS uuid \
                 LANGUAGE sql STABLE AS $$ \
                 SELECT id FROM organization.org_units WHERE kind = 'root' $$; \
             CREATE SCHEMA org_scope_test; \
             CREATE TABLE org_scope_test.t (id uuid PRIMARY KEY, org_unit_id uuid NOT NULL, code text); \
             ALTER TABLE org_scope_test.t ENABLE ROW LEVEL SECURITY; \
             ALTER TABLE org_scope_test.t FORCE ROW LEVEL SECURITY; \
             CREATE POLICY t_org_isolation ON org_scope_test.t FOR ALL \
                 USING (org_unit_id = ANY(string_to_array(current_setting('app.scope_unit_ids', true), ',')::uuid[])) \
                 WITH CHECK (org_unit_id = ANY(string_to_array(current_setting('app.scope_unit_ids', true), ',')::uuid[])); \
             CREATE ROLE org_scope_app LOGIN PASSWORD 'orgpw'; \
             GRANT USAGE ON SCHEMA organization, org_scope_test TO org_scope_app; \
             GRANT SELECT ON organization.org_units TO org_scope_app; \
             GRANT SELECT, INSERT, UPDATE, DELETE ON org_scope_test.t TO org_scope_app;",
        )
        .execute(admin)
        .await
        .unwrap();

        sqlx::query(
            "INSERT INTO organization.org_units (id, kind, parent_id, code, name) VALUES \
                 ($1, 'root',   NULL, 'root',  'Tenant root'), \
                 ($2, 'company', $1,   'CO',    'Company'), \
                 ($3, 'branch',  $2,   'BR',    'Branch'), \
                 ($4, 'company', $1,   'OTHER', 'Other Company')",
        )
        .bind(root)
        .bind(company)
        .bind(branch)
        .bind(other)
        .execute(admin)
        .await
        .unwrap();
    }

    #[tokio::test]
    async fn resolver_and_fence_shape() {
        let Some(dsn) = dsn() else { eprintln!("skipping: set BACKBONE_ORM_RLS_DSN"); return };
        let root = Uuid::new_v4();
        let company = Uuid::new_v4();
        let branch = Uuid::new_v4();
        let other = Uuid::new_v4();
        let admin = admin_pool(&dsn).await;
        setup(&admin, root, company, branch, other).await;

        // Seed as superuser (bypasses RLS).
        sqlx::query("INSERT INTO org_scope_test.t (id, org_unit_id, code) VALUES ($1,$2,'CO-WH'), ($3,$4,'BR-WH'), ($5,$6,'OTHER-WH')")
            .bind(Uuid::new_v4()).bind(company)
            .bind(Uuid::new_v4()).bind(branch)
            .bind(Uuid::new_v4()).bind(other)
            .execute(&admin).await.unwrap();

        // Resolve from the branch node: subtree() DESCENDS only, so the scope is the branch's
        // own subtree (itself) plus the root node — the parent company is NOT auto-included;
        // seeing it requires entitlement. Legacy bridge = the governing company.
        let mut conn = admin.acquire().await.unwrap();
        let scope = resolve_org_scope(&mut conn, branch, &[]).await.unwrap();
        let mut got = scope.scope_unit_ids().to_vec();
        got.sort_unstable();
        let mut want = vec![branch, root];
        want.sort_unstable();
        assert_eq!(got, want);
        assert_eq!(scope.acting_unit_id(), branch);
        assert_eq!(scope.legacy_company_id(), Some(company));

        // Union entitlement: branch acting + sister company entitled.
        let scope_union = resolve_org_scope(&mut conn, branch, &[other]).await.unwrap();
        assert!(scope_union.scope_unit_ids().contains(&other));

        // Unknown acting node is a loud error, not a narrower scope.
        let ghost = resolve_org_scope(&mut conn, Uuid::new_v4(), &[]).await;
        assert!(ghost.is_err());

        // The request scope fences queries on the app role's pool. The scoped helper (not a raw
        // pool fetch) is the interesting path: it must route onto the request connection the
        // scope bound, which carries the fence variables.
        let pool = app_pool(&dsn).await;
        let codes: Vec<String> = with_org_request_scope(&pool, scope, async {
            crate::company_scope::fetch_all_scoped(
                &pool,
                sqlx::query_as::<_, (String,)>("SELECT code FROM org_scope_test.t ORDER BY code"),
            )
            .await
            .unwrap()
            .into_iter()
            .map(|r| r.0)
            .collect()
        })
        .await
        .unwrap();
        assert_eq!(codes, ["BR-WH"], "branch session sees only its subtree (+ shared root rows), not the company or sister-company rows");

        // After the scope, the pooled connection is clean (fail-closed for the next acquire).
        let mut after = pool.acquire().await.unwrap();
        let setting: String =
            sqlx::query_scalar("SELECT current_setting('app.scope_unit_ids', true)")
                .fetch_one(&mut *after)
                .await
                .unwrap();
        assert_eq!(setting, "", "scope leaked onto the pooled connection");
    }
}
