//! Request audit context: the attribution channel of data-change audit capture (ADR-0025).
//!
//! The auditlog module's capture function attributes a row change to WHO made it and WHERE it
//! came from by reading session variables off the connection the write rides:
//!
//! - `app.actor` — the authenticated principal (the token's `sub`). Unset reads as NULL and the
//!   capture function falls back to `'system'`, so background jobs and unattributed writes stay
//!   distinguishable from named users without failing.
//! - `app.correlation_id` — the request correlation id (honored from `X-Correlation-ID`, else
//!   minted per request). This is the join key between an audit row and the request that caused
//!   it — logs, responses and audit rows carry the same value.
//! - `app.client_ip`, `app.user_agent`, `app.http_method`, `app.resource_path` — the request
//!   facts, for after-the-fact triage of a suspicious change.
//!
//! All six are empty-string-when-unset on the wire: every reader wraps them in
//! `nullif(current_setting(..., true), '')`, so an unset variable reads NULL there. Setting an
//! empty string is therefore indistinguishable from never having set the variable — there is no
//! partial-trust state to reason about.
//!
//! [`bind_on`](RequestAuditContext::bind_on) is the application half of the channel; the
//! composing service's guard builds the context (actor off a signed token, the rest off the
//! request) and the request scope carries it, exactly like the fence variables. The database
//! half — the trigger reading them — is owned by the composed auditlog module.

use sqlx::PgConnection;

/// Every session variable of the audit channel, in bind and reset order.
///
/// Public so request-scope wrappers can reset the whole channel with one inventory (the same
/// reason `with_org_request_scope` resets every fence variable it may have set, unconditionally
/// — pool hygiene is cheaper than proving which variables a given request set).
pub const AUDIT_CONTEXT_VARS: [&str; 6] = [
    "app.actor",
    "app.correlation_id",
    "app.client_ip",
    "app.user_agent",
    "app.http_method",
    "app.resource_path",
];

/// Who and where one request's writes are attributed to (ADR-0025).
///
/// Built by the composing service's guard — the actor off a signed token, never the request
/// body — and carried by the request scope alongside the fence variables, so every write of the
/// request (a trigger fires on it) reads the same attribution. All fields are
/// empty-string-when-unknown; see the [module](self) docs for how readers treat that.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct RequestAuditContext {
    /// The authenticated principal the capture function records as the actor. Empty falls back
    /// to `'system'` at capture time.
    pub actor: String,
    /// The request correlation id — the join key between audit rows and the request that
    /// caused them.
    pub correlation_id: String,
    /// The client's IP as the edge proxy reported it (`X-Forwarded-For`'s first entry).
    pub client_ip: String,
    /// The request's `User-Agent`.
    pub user_agent: String,
    /// The HTTP method, upper-case as the router saw it.
    pub http_method: String,
    /// The request path (no query string) as the router saw it.
    pub resource_path: String,
}

impl RequestAuditContext {
    /// A context with only an actor — the minimum attribution a guarded request carries; every
    /// other field stays unset (reads NULL at capture time).
    pub fn new(actor: impl Into<String>) -> Self {
        Self {
            actor: actor.into(),
            ..Self::default()
        }
    }

    /// The (variable, value) pairs of the whole channel, in [`AUDIT_CONTEXT_VARS`] order —
    /// the wire form bind and tests walk.
    pub fn pairs(&self) -> [(&'static str, &str); 6] {
        [
            ("app.actor", self.actor.as_str()),
            ("app.correlation_id", self.correlation_id.as_str()),
            ("app.client_ip", self.client_ip.as_str()),
            ("app.user_agent", self.user_agent.as_str()),
            ("app.http_method", self.http_method.as_str()),
            ("app.resource_path", self.resource_path.as_str()),
        ]
    }

    /// Bind every audit variable on a connection.
    ///
    /// `local = true` binds transaction-locally (the twin of
    /// [`bind_org_scope_on`](crate::org_scope::bind_org_scope_on), for hand-written write
    /// services that manage their own transaction); `local = false` binds at session level on
    /// a request-dedicated connection — the form the request scope uses, because the variables
    /// must outlive any single statement for the whole request.
    ///
    /// # Errors
    /// Returns the sqlx error if any `set_config` fails (connection lost mid-bind).
    pub async fn bind_on(&self, conn: &mut PgConnection, local: bool) -> Result<(), sqlx::Error> {
        for (var, value) in self.pairs() {
            sqlx::query("SELECT set_config($1, $2, $3)")
                .bind(var)
                .bind(value)
                .bind(local)
                .execute(&mut *conn)
                .await?;
        }
        Ok(())
    }
}
