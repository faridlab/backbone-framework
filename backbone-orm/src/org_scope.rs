//! Org-tree request scope for the entitlement-union RLS fence (ADR-0028/0029).
//!
//! ADR-0028 replaces the single `company_id` scoping key with `org_unit_id`: one org tree per
//! tenant database (root / company / branch nodes), and a session sees the UNION of the subtrees
//! under every node it is entitled to — always including the root node, which owns tenant-wide
//! shared rows. The database half is the policy
//! `org_unit_id = ANY(string_to_array(current_setting('app.scope_unit_ids', true), ',')::uuid[])`;
//! this module is the application half that resolves a session's entitled ids and carries them
//! for the duration of a request.
//!
//! Session variables set by an org request scope, in full:
//! - `app.scope_unit_ids` — the entitlement-union fence (read by org policies).
//! - `app.company_id` — the legacy equality fence during the re-key transition, resolved from
//!   the acting node's company ancestry.
//! - `app.acting_unit_id` — where new records land: the column DEFAULT
//!   `nullif(current_setting('app.acting_unit_id', true), '')::uuid` on decorated tables
//!   (ADR-0029) resolves INSERTs that omit `org_unit_id`. Unset/empty → NULL → NOT NULL
//!   violation: an insert outside a scope fails loud, never silently unscoped.
//! - the six `app.*` audit variables of [`crate::audit_context`] (actor, correlation id,
//!   request facts) when the request carries a
//!   [`RequestAuditContext`](crate::audit_context::RequestAuditContext) — set by the audited
//!   twin [`with_org_request_scope_and_audit`], on the same request-dedicated connection, so
//!   the auditlog capture function's triggers read attribution off every write of the request.
//!
//! During the module-by-module re-key both fences are live at once: some tables still read
//! `app.company_id` (ADR-0008 equality fence), org-re-keyed tables read `app.scope_unit_ids`.
//! [`with_org_request_scope`] therefore sets ALL THREE session variables on one request-dedicated
//! connection. When the last company-fenced table is re-keyed, the legacy bridge retires with
//! the old fence.
//!
//! The scope binds the same `REQUEST_CONN` task-local as
//! [`with_request_scope`](crate::company_scope::with_request_scope): every scoped execute helper
//! in this crate — and therefore every generated repository — runs on that connection and
//! inherits the fence variables without a single call-site change.
//!
//! **The task-local is not the fence.** RLS is. Unscoped statements see the variables unset and
//! match zero rows — fail-closed, identical to the ADR-0008 contract.

use crate::audit_context::{AUDIT_CONTEXT_VARS, RequestAuditContext};
use sqlx::PgPool;
use std::fmt;
use std::future::Future;
use std::sync::Arc;
use tokio::sync::Mutex;
use uuid::Uuid;

/// The fence variables an org request scope sets — the reset inventory's first half. Private:
/// callers reset through the scope wrappers, which clear fence AND audit variables together.
const ORG_FENCE_VARS: [&str; 3] = ["app.scope_unit_ids", "app.company_id", "app.acting_unit_id"];

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
    /// A single-company scope for paths that cannot run the resolver — chiefly composition
    /// seams minting records on a node the request already pinned (ADR-0029's decorator makes
    /// those inserts resolve their `org_unit_id` from `app.acting_unit_id`).
    ///
    /// **Precondition: `unit` must be a COMPANY node** (or whatever node kind the caller's
    /// rows anchor on, with `legacy_company_id` semantics in mind). [`resolve_org_scope`]
    /// derives the legacy `app.company_id` by walking ancestors from the acting node; this
    /// constructor sets it verbatim, so a BRANCH handed here would bind a legacy variable
    /// matching no company-fenced row and fail closed on every not-yet-stripped table.
    ///
    /// The scope ids are exactly `[unit]` — no root-shared rows, no sibling subtrees. That is
    /// fail-narrow (the same property as [`execute_unit_scoped`]): inserts land on the unit
    /// and reads see only the unit's own rows. Paths needing the session's entitlement union
    /// must resolve a real scope instead.
    pub fn for_company_unit(unit: Uuid) -> Self {
        Self {
            scope_unit_ids: vec![unit],
            acting_unit_id: unit,
            legacy_company_id: Some(unit),
        }
    }

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
/// Sets `app.scope_unit_ids` (the entitlement-union fence), the legacy `app.company_id`
/// (equality fence, resolved from the acting node's company ancestry), and `app.acting_unit_id`
/// (the acting-unit DEFAULT source for inserts on decorated tables, ADR-0029) at the session
/// level, so org-re-keyed and not-yet-re-keyed tables are both fenced correctly for the whole
/// request — including ID-only lookups, which ride the connection rather than the query text.
///
/// Mirrors [`with_request_scope`](crate::company_scope::with_request_scope)'s reset discipline:
/// all three variables are cleared unconditionally before the connection returns to the pool,
/// even when a `REQUEST_CONN` clone outlives the scope — a clone that runs queries after the
/// reset does so unscoped (fail-closed), never with the previous session's scope.
pub async fn with_org_request_scope<F, R>(pool: &PgPool, scope: OrgScope, f: F) -> Result<R, sqlx::Error>
where
    F: Future<Output = R>,
{
    with_org_request_scope_internal(pool, scope, None, f).await
}

/// [`with_org_request_scope`] plus the request's audit attribution (ADR-0025): the six
/// [`RequestAuditContext`] variables are bound on the same request-dedicated connection, so
/// every write of the request — including the ones that fire the auditlog capture function's
/// triggers — reads the same actor and request facts off the connection it rides.
///
/// This is the wrapper a guarded route runs: the guard resolves the scope off the token, builds
/// the audit context off the token's `sub` and the request itself, and the two channels travel
/// together for the whole request. The reset discipline clears fence AND audit variables
/// unconditionally: a pooled connection must never carry the previous request's attribution
/// into the next one's audit rows.
pub async fn with_org_request_scope_and_audit<F, R>(
    pool: &PgPool,
    scope: OrgScope,
    audit: RequestAuditContext,
    f: F,
) -> Result<R, sqlx::Error>
where
    F: Future<Output = R>,
{
    with_org_request_scope_internal(pool, scope, Some(&audit), f).await
}

async fn with_org_request_scope_internal<F, R>(
    pool: &PgPool,
    scope: OrgScope,
    audit: Option<&RequestAuditContext>,
    f: F,
) -> Result<R, sqlx::Error>
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
    sqlx::query("SELECT set_config('app.acting_unit_id', $1, false)")
        .bind(scope.acting_unit_id.to_string())
        .execute(&mut *conn)
        .await?;
    if let Some(audit) = audit {
        audit.bind_on(&mut conn, false).await?;
    }

    let holder = Arc::new(Mutex::new(conn));
    let scope_arc = Arc::new(scope);
    let result = ORG_SCOPE.scope(scope_arc.clone(), async {
        crate::company_scope::with_company_scope_internal(scope_arc.legacy_company_id, async {
            crate::company_scope::with_request_conn_internal(holder.clone(), f).await
        })
        .await
    })
    .await;

    // Unconditional reset of every variable the scope may have set — fence always, audit when
    // the request carried a context (resetting anyway when it did not costs six cheap
    // set_config calls and keeps the inventory one list; see with_request_scope for the
    // lingering-clone reasoning — the same contract applies to every variable).
    {
        let mut guard = holder.lock().await;
        for var in ORG_FENCE_VARS.into_iter().chain(AUDIT_CONTEXT_VARS) {
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
/// [`bind_company_on`](crate::company_scope::bind_company_on)). Binds all three session
/// variables, `app.acting_unit_id` included, so an insert inside the transaction can rely on
/// the acting-unit DEFAULT.
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
    sqlx::query("SELECT set_config('app.acting_unit_id', $1, true)")
        .bind(scope.acting_unit_id.to_string())
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

/// Tenant-agnostic `execute` for hand-written module SQL (ADR-0029): ride the request-dedicated
/// connection when one is bound — carrying whatever fence variables the COMPOSING service's scope
/// set (`with_org_request_scope` / `with_request_scope`) — otherwise execute plainly on the pool.
///
/// Unlike [`execute_unit_scoped`] this invents no scope of its own: a module that knows nothing
/// about tenancy must not fabricate a unit or a company. Under a composer's request scope the
/// database fence owns isolation; with no scope bound this is a plain unfenced execute.
pub async fn execute_scoped<'q>(
    pool: &PgPool,
    query: sqlx::query::Query<'q, sqlx::Postgres, sqlx::postgres::PgArguments>,
) -> Result<sqlx::postgres::PgQueryResult, sqlx::Error> {
    if let Some(conn) = crate::company_scope::current_request_conn() {
        let mut g = conn.lock().await;
        return query.execute(&mut **g).await;
    }
    query.execute(pool).await
}

/// Tenant-agnostic `fetch_optional` for an untyped row query — the read twin of
/// [`execute_scoped`], same connection discipline: request-dedicated connection when bound,
/// plain pool otherwise, no scope invented.
pub async fn fetch_optional_row_scoped<'q>(
    pool: &PgPool,
    query: sqlx::query::Query<'q, sqlx::Postgres, sqlx::postgres::PgArguments>,
) -> Result<Option<sqlx::postgres::PgRow>, sqlx::Error> {
    if let Some(conn) = crate::company_scope::current_request_conn() {
        let mut g = conn.lock().await;
        return query.fetch_optional(&mut **g).await;
    }
    query.fetch_optional(pool).await
}

#[cfg(test)]
mod tests {
    //! Gated on `BACKBONE_ORM_RLS_DSN` (a superuser DSN). Self-contained: builds a minimal org
    //! spine + an org-fenced table + an app role, then proves the resolver and the request
    //! scope against them — the same shapes the organization/inventory migrations emit.
    use super::{resolve_org_scope, with_org_request_scope, with_org_request_scope_and_audit};
    use crate::audit_context::RequestAuditContext;
    use sqlx::postgres::PgPoolOptions;
    use sqlx::PgPool;
    use uuid::Uuid;

    fn dsn() -> Option<String> {
        std::env::var("BACKBONE_ORM_RLS_DSN").ok()
    }

    async fn admin_pool(dsn: &str) -> PgPool {
        PgPoolOptions::new().max_connections(4).connect(dsn).await.unwrap()
    }

    async fn app_pool(dsn: &str, role: &str) -> PgPool {
        let after_at = dsn.rsplit('@').next().unwrap();
        let url = format!("postgresql://{role}:orgpw@{after_at}");
        PgPoolOptions::new().max_connections(1).connect(&url).await.unwrap()
    }

    /// Mint the per-run role name — a fixed name breaks on shared dev clusters (`DROP ROLE`
    /// fails while the role holds grants in another database), a fresh one can never collide.
    fn role_name() -> String {
        format!("org_scope_app_{}", &Uuid::new_v4().simple().to_string()[..8])
    }

    async fn setup(admin: &PgPool, role: &str, root: Uuid, company: Uuid, branch: Uuid, other: Uuid) {
        sqlx::raw_sql(&format!(
            "DROP SCHEMA IF EXISTS organization CASCADE; \
             DROP SCHEMA IF EXISTS org_scope_test CASCADE; \
             CREATE SCHEMA organization; \
             CREATE TABLE organization.org_units ( \
                 id uuid PRIMARY KEY, kind text NOT NULL, parent_id uuid, \
                 code text, name text NOT NULL, metadata jsonb NOT NULL DEFAULT '{{}}' \
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
             CREATE ROLE {role} LOGIN PASSWORD 'orgpw'; \
             GRANT USAGE ON SCHEMA organization, org_scope_test TO {role}; \
             GRANT SELECT ON organization.org_units TO {role}; \
             GRANT SELECT, INSERT, UPDATE, DELETE ON org_scope_test.t TO {role};",
        ))
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
        let role = role_name();
        let admin = admin_pool(&dsn).await;
        setup(&admin, &role, root, company, branch, other).await;

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
        let pool = app_pool(&dsn, &role).await;
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

    /// The audit-context channel of the request scope (ADR-0025): the audited twin binds the
    /// six `app.*` attribution variables on the request-dedicated connection, a row-level
    /// trigger on a write through the scoped helpers reads them off that connection, and every
    /// variable — fence AND audit — is cleared before the connection returns to the pool. The
    /// unaudited twin leaves the audit variables unset (the capture function's `'system'`
    /// fallback reads exactly that: empty).
    ///
    /// Runs in its own disposable database (dropped at the end), because the spine-building
    /// sibling test above recreates the `organization` schema and would race this one's if they
    /// shared a database — cargo runs test fns concurrently.
    #[tokio::test]
    async fn audit_context_binds_rides_and_clears_with_the_scope() {
        let Some(maintenance_dsn) = dsn() else {
            eprintln!("skipping: set BACKBONE_ORM_RLS_DSN");
            return;
        };
        const PROOF_DB: &str = "backbone_orm_audit_ctx_probe";
        let proof_dsn = {
            let (before_db, ..) = maintenance_dsn.rsplit_once('/').unwrap();
            format!("{before_db}/{PROOF_DB}")
        };
        let maintenance = admin_pool(&maintenance_dsn).await;
        // Each statement rides its own simple-protocol query — CREATE/DROP DATABASE may not run
        // inside a transaction block.
        sqlx::raw_sql(&format!("DROP DATABASE IF EXISTS {PROOF_DB} WITH (FORCE)"))
            .execute(&maintenance)
            .await
            .unwrap();
        sqlx::raw_sql(&format!("CREATE DATABASE {PROOF_DB}"))
            .execute(&maintenance)
            .await
            .unwrap();
        let admin = admin_pool(&proof_dsn).await;

        let root = Uuid::new_v4();
        let company = Uuid::new_v4();
        let role = role_name();
        // The spine + fenced table of the sibling test, plus the audit probe: a table and an
        // AFTER INSERT trigger capturing two of the attribution variables — the minimal shape
        // of the auditlog module's capture function, whose contract this pins without the
        // framework crate depending on a domain module.
        sqlx::raw_sql(&format!(
            "CREATE SCHEMA organization; \
             CREATE TABLE organization.org_units ( \
                 id uuid PRIMARY KEY, kind text NOT NULL, parent_id uuid, \
                 code text, name text NOT NULL, metadata jsonb NOT NULL DEFAULT '{{}}' \
             ); \
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
             CREATE TABLE org_scope_test.t (id uuid PRIMARY KEY, org_unit_id uuid NOT NULL \
                 DEFAULT nullif(current_setting('app.acting_unit_id', true), '')::uuid, code text); \
             ALTER TABLE org_scope_test.t ENABLE ROW LEVEL SECURITY; \
             ALTER TABLE org_scope_test.t FORCE ROW LEVEL SECURITY; \
             CREATE POLICY t_org_isolation ON org_scope_test.t FOR ALL \
                 USING (org_unit_id = ANY(string_to_array(current_setting('app.scope_unit_ids', true), ',')::uuid[])) \
                 WITH CHECK (org_unit_id = ANY(string_to_array(current_setting('app.scope_unit_ids', true), ',')::uuid[])); \
             CREATE TABLE org_scope_test.audit_probe ( \
                 id bigserial PRIMARY KEY, actor text NOT NULL, correlation_id text NOT NULL \
             ); \
             CREATE FUNCTION org_scope_test.capture_probe() RETURNS trigger LANGUAGE plpgsql AS $$ \
                 BEGIN \
                     INSERT INTO org_scope_test.audit_probe (actor, correlation_id) \
                     VALUES (current_setting('app.actor', true), \
                             current_setting('app.correlation_id', true)); \
                     RETURN NULL; \
                 END $$; \
             CREATE TRIGGER t_audit_probe AFTER INSERT ON org_scope_test.t \
                 FOR EACH ROW EXECUTE FUNCTION org_scope_test.capture_probe(); \
             CREATE ROLE {role} LOGIN PASSWORD 'orgpw'; \
             GRANT USAGE ON SCHEMA organization, org_scope_test TO {role}; \
             GRANT SELECT ON organization.org_units TO {role}; \
             GRANT SELECT, INSERT ON org_scope_test.t TO {role}; \
             GRANT INSERT ON org_scope_test.audit_probe TO {role}; \
             GRANT USAGE, SELECT ON SEQUENCE org_scope_test.audit_probe_id_seq TO {role};",
        ))
        .execute(&admin)
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO organization.org_units (id, kind, parent_id, code, name) VALUES \
                 ($1, 'root',    NULL, 'ROOT', 'Tenant root'), \
                 ($2, 'company', $1,   'CO',   'Company')",
        )
        .bind(root)
        .bind(company)
        .execute(&admin)
        .await
        .unwrap();

        let pool = app_pool(&proof_dsn, &role).await;
        let scope = {
            let mut conn = admin.acquire().await.unwrap();
            resolve_org_scope(&mut conn, company, &[]).await.unwrap()
        };

        // 1. The audited twin: every attribution variable is set on the connection writes ride
        //    (inventory proof, same style as the fence-variable inventory above), and a trigger
        //    on a real insert reads them — the seam the auditlog capture function depends on.
        let audit = RequestAuditContext {
            actor: "user-77".to_string(),
            correlation_id: "corr-77".to_string(),
            client_ip: "203.0.113.7".to_string(),
            user_agent: "probe-agent/1.0".to_string(),
            http_method: "POST".to_string(),
            resource_path: "/api/v1/probe/widgets".to_string(),
        };
        let inserted = Uuid::new_v4();
        let settings: Vec<(String, String)> = with_org_request_scope_and_audit(
            &pool,
            scope.clone(),
            audit.clone(),
            async {
                crate::company_scope::execute_scoped(
                    &pool,
                    sqlx::query("INSERT INTO org_scope_test.t (id, code) VALUES ($1, 'AUDITED')")
                        .bind(inserted),
                )
                .await
                .unwrap();
                crate::company_scope::fetch_all_scoped(
                    &pool,
                    sqlx::query_as::<_, (String, String)>(
                        "SELECT s.name, current_setting(s.name, true) FROM unnest(ARRAY[\
                         'app.actor','app.correlation_id','app.client_ip','app.user_agent',\
                         'app.http_method','app.resource_path']) AS s(name)",
                    ),
                )
                .await
                .unwrap()
            },
        )
        .await
        .unwrap();
        for (var, want) in audit.pairs() {
            let got = settings
                .iter()
                .find(|(n, _)| n == var)
                .map(|(_, v)| v.as_str())
                .unwrap_or_else(|| panic!("{var} missing from the inventory query"));
            assert_eq!(got, want, "{var} must ride the request connection");
        }
        // The fence still works under the audited twin: the acting-unit DEFAULT filled the row.
        let landed: Uuid = sqlx::query_scalar("SELECT org_unit_id FROM org_scope_test.t WHERE id = $1")
            .bind(inserted)
            .fetch_one(&admin)
            .await
            .unwrap();
        assert_eq!(landed, company, "audit channel must not disturb the fence");
        let probe: (String, String) =
            sqlx::query_as("SELECT actor, correlation_id FROM org_scope_test.audit_probe ORDER BY id DESC LIMIT 1")
                .fetch_one(&admin)
                .await
                .unwrap();
        assert_eq!(probe, ("user-77".to_string(), "corr-77".to_string()),
            "a row-level trigger on the insert must read the bound attribution");

        // 2. Pool hygiene: after the scope, no audit variable survives on the pooled connection.
        let mut after = pool.acquire().await.unwrap();
        for var in crate::audit_context::AUDIT_CONTEXT_VARS {
            let v: String = sqlx::query_scalar("SELECT current_setting($1, true)")
                .bind(var)
                .fetch_one(&mut *after)
                .await
                .unwrap();
            assert_eq!(v, "", "{var} leaked onto the pooled connection");
        }
        drop(after);

        // 3. The unaudited twin leaves the channel unset — the capture function's `'system'`
        //    fallback and NULL columns read exactly this empty wire form.
        let plain = Uuid::new_v4();
        with_org_request_scope(&pool, scope, async {
            crate::company_scope::execute_scoped(
                &pool,
                sqlx::query("INSERT INTO org_scope_test.t (id, code) VALUES ($1, 'PLAIN')")
                    .bind(plain),
            )
            .await
            .unwrap();
        })
        .await
        .unwrap();
        let probe: (String, String) =
            sqlx::query_as("SELECT actor, correlation_id FROM org_scope_test.audit_probe ORDER BY id DESC LIMIT 1")
                .fetch_one(&admin)
                .await
                .unwrap();
        assert_eq!(probe, (String::new(), String::new()),
            "without an audit context the channel reads empty, never the previous request's");

        // Zero residue: schemas + role in the proof database, then the database itself.
        sqlx::raw_sql(&format!(
            "DROP SCHEMA organization CASCADE; DROP SCHEMA org_scope_test CASCADE; DROP ROLE {role};"
        ))
        .execute(&admin)
        .await
        .unwrap();
        pool.close().await;
        admin.close().await;
        sqlx::raw_sql(&format!("DROP DATABASE IF EXISTS {PROOF_DB} WITH (FORCE);"))
            .execute(&maintenance)
            .await
            .unwrap();
        maintenance.close().await;
    }
}
