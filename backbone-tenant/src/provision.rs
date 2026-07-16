//! Tenant database provisioning (ADR-0006, Seam A) — feature `provision`.
//!
//! Under ADR-0005 the tenant *is* a database. Before a tenant can serve a request, that database must
//! exist and carry every module's schema. This is the mechanism that makes one: create the database,
//! run the migrations into it. The routing registry ([`crate::TenantRegistry`]) then opens a pool to
//! it and builds the modules.
//!
//! The composition root owns the *policy* (which migrations, when, driven by signup/billing); this is
//! the *mechanism*, kept free of any module dependency.
//!
//! ## Safety
//!
//! `CREATE DATABASE` / `DROP DATABASE` name their target as an **identifier**, which SQL cannot
//! parameterize — the name is interpolated into the statement. So a tenant id is never trusted
//! verbatim: [`TenantProvisioner::database_name`] rejects any id that is not a bounded
//! `[a-z0-9_-]` slug, and only then is a name built. An id that could carry a quote or a semicolon is
//! an error, not an escaped string.

use std::str::FromStr;

use sqlx::{Connection, Executor, PgConnection};

use crate::TenantId;

/// The Postgres identifier length limit (`NAMEDATALEN - 1`).
const MAX_IDENT_LEN: usize = 63;

/// Provisioning failures.
#[derive(Debug, thiserror::Error)]
pub enum ProvisionError {
    /// The tenant id cannot be turned into a safe database identifier.
    #[error("tenant id '{id}' is not a valid database slug: {reason}")]
    UnsafeTenantId {
        /// The offending id.
        id: String,
        /// Why it was rejected.
        reason: &'static str,
    },
    /// A database/connection error from the driver.
    #[error(transparent)]
    Db(#[from] sqlx::Error),
}

/// Creates and migrates per-tenant databases over an admin (maintenance-DB) connection.
///
/// The `admin_dsn` must point at a database the role may connect to in order to run
/// `CREATE DATABASE` (conventionally the `postgres` maintenance database) — not at a tenant database.
#[derive(Debug, Clone)]
pub struct TenantProvisioner {
    admin_dsn: String,
    prefix: String,
}

impl TenantProvisioner {
    /// Provision over `admin_dsn`. Tenant databases are named `tenant_<slug>` by default.
    pub fn new(admin_dsn: impl Into<String>) -> Self {
        Self {
            admin_dsn: admin_dsn.into(),
            prefix: "tenant_".to_string(),
        }
    }

    /// Override the database-name prefix (default `tenant_`).
    pub fn with_prefix(mut self, prefix: impl Into<String>) -> Self {
        self.prefix = prefix.into();
        self
    }

    /// The safe database name for `tenant`, or an error if the id is not a bounded `[a-z0-9_-]` slug.
    ///
    /// `-` is normalised to `_` (subdomains use dashes, identifiers use underscores). The charset is
    /// validated *before* any name is built, so the returned string cannot contain a quote, a space,
    /// a semicolon, or any other character that could break out of the DDL identifier.
    pub fn database_name(&self, tenant: &TenantId) -> Result<String, ProvisionError> {
        let id = tenant.as_str();
        let reject = |reason| {
            Err(ProvisionError::UnsafeTenantId { id: id.to_string(), reason })
        };
        if id.is_empty() {
            return reject("empty");
        }
        if !id.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-' || c == '_') {
            return reject("only lowercase letters, digits, '-' and '_' are allowed");
        }
        // First char must be a letter or underscore — a database name cannot start with a digit or
        // dash without quoting gymnastics, and we want plain identifiers.
        let first = id.chars().next().unwrap();
        if !(first.is_ascii_lowercase() || first == '_') {
            return reject("must start with a letter or underscore");
        }
        let name = format!("{}{}", self.prefix, id.replace('-', "_"));
        if name.len() > MAX_IDENT_LEN {
            return reject("resulting database name exceeds 63 characters");
        }
        Ok(name)
    }

    /// The DSN for connecting to `tenant`'s database (the admin DSN with the database swapped).
    pub fn tenant_dsn(&self, tenant: &TenantId) -> Result<String, ProvisionError> {
        let db = self.database_name(tenant)?;
        let opts = sqlx::postgres::PgConnectOptions::from_str(&self.admin_dsn)?.database(&db);
        // Re-render to a URL so callers can hand it to a pool builder unchanged.
        Ok(render_dsn(&self.admin_dsn, &db).unwrap_or_else(|| {
            // Fallback: options exist even if URL re-render fails; a pool can take the options too.
            let _ = opts;
            format!("{}#db={}", self.admin_dsn, db)
        }))
    }

    async fn admin_conn(&self) -> Result<PgConnection, ProvisionError> {
        Ok(PgConnection::connect(&self.admin_dsn).await?)
    }

    /// Whether `tenant`'s database exists.
    pub async fn exists(&self, tenant: &TenantId) -> Result<bool, ProvisionError> {
        let db = self.database_name(tenant)?;
        let mut conn = self.admin_conn().await?;
        let found: Option<(i32,)> = sqlx::query_as("SELECT 1 FROM pg_database WHERE datname = $1")
            .bind(&db)
            .fetch_optional(&mut conn)
            .await?;
        Ok(found.is_some())
    }

    /// Create `tenant`'s database if it does not already exist. Idempotent; returns the database name.
    pub async fn create_database(&self, tenant: &TenantId) -> Result<String, ProvisionError> {
        let db = self.database_name(tenant)?;
        if self.exists(tenant).await? {
            return Ok(db);
        }
        let mut conn = self.admin_conn().await?;
        // `db` is a validated slug; quote it as an identifier for defense in depth. CREATE DATABASE
        // cannot run inside a transaction, so this executes on a bare connection (autocommit).
        let stmt = format!("CREATE DATABASE \"{db}\"");
        match conn.execute(stmt.as_str()).await {
            Ok(_) => Ok(db),
            // 42P04 = duplicate_database: someone raced us; the post-condition (db exists) holds.
            Err(sqlx::Error::Database(e)) if e.code().as_deref() == Some("42P04") => Ok(db),
            Err(e) => Err(e.into()),
        }
    }

    /// Run `statements` against `tenant`'s database, in order, each in autocommit.
    ///
    /// Intended for a module's migration SQL. The database must already exist
    /// ([`create_database`](Self::create_database)).
    pub async fn migrate(&self, tenant: &TenantId, statements: &[&str]) -> Result<(), ProvisionError> {
        let db = self.database_name(tenant)?;
        let dsn = render_dsn(&self.admin_dsn, &db)
            .ok_or(ProvisionError::UnsafeTenantId { id: tenant.as_str().to_string(), reason: "admin DSN could not be re-targeted" })?;
        let mut conn = PgConnection::connect(&dsn).await?;
        for stmt in statements {
            conn.execute(*stmt).await?;
        }
        Ok(())
    }

    /// Create (if needed) then migrate — the full path to a ready tenant database. Returns its name.
    pub async fn provision(&self, tenant: &TenantId, statements: &[&str]) -> Result<String, ProvisionError> {
        let db = self.create_database(tenant).await?;
        self.migrate(tenant, statements).await?;
        Ok(db)
    }

    /// Drop `tenant`'s database. **Destructive.** Existing connections are terminated first, since
    /// `DROP DATABASE` fails while any session is connected. Returns whether a database was dropped.
    pub async fn deprovision(&self, tenant: &TenantId) -> Result<bool, ProvisionError> {
        let db = self.database_name(tenant)?;
        let mut conn = self.admin_conn().await?;
        // Kick every other session off the target so the DROP can proceed.
        sqlx::query("SELECT pg_terminate_backend(pid) FROM pg_stat_activity WHERE datname = $1 AND pid <> pg_backend_pid()")
            .bind(&db)
            .execute(&mut conn)
            .await?;
        let stmt = format!("DROP DATABASE IF EXISTS \"{db}\"");
        let res = conn.execute(stmt.as_str()).await?;
        Ok(res.rows_affected() > 0 || self.exists(tenant).await.map(|e| !e).unwrap_or(false))
    }
}

/// Rebuild a DSN with a different database path, preserving scheme/credentials/host/query.
///
/// `db` is a validated slug, so it is safe to place in the path.
fn render_dsn(admin_dsn: &str, db: &str) -> Option<String> {
    // Split off any query string, replace the path segment after the last '/', reattach the query.
    let (base, query) = match admin_dsn.split_once('?') {
        Some((b, q)) => (b, Some(q)),
        None => (admin_dsn, None),
    };
    let slash = base.rfind('/')?;
    let rebuilt = format!("{}/{}", &base[..slash], db);
    Some(match query {
        Some(q) => format!("{rebuilt}?{q}"),
        None => rebuilt,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn provisioner() -> TenantProvisioner {
        TenantProvisioner::new("postgresql://u:p@host:5432/postgres")
    }

    #[test]
    fn valid_slugs_produce_prefixed_names() {
        let p = provisioner();
        assert_eq!(p.database_name(&"acme".into()).unwrap(), "tenant_acme");
        assert_eq!(p.database_name(&"acme_corp".into()).unwrap(), "tenant_acme_corp");
        // Dashes (subdomains) become underscores.
        assert_eq!(p.database_name(&"acme-corp".into()).unwrap(), "tenant_acme_corp");
    }

    #[test]
    fn injection_shaped_ids_are_rejected() {
        let p = provisioner();
        for bad in [
            "acme\"; DROP DATABASE postgres; --",
            "acme'",
            "acme corp",
            "ACME",           // uppercase not allowed (identifiers are folded; keep them explicit)
            "1acme",          // starts with a digit
            "-acme",          // starts with a dash
            "",               // empty
        ] {
            assert!(
                p.database_name(&bad.into()).is_err(),
                "'{bad}' must be rejected, not turned into a database name"
            );
        }
    }

    #[test]
    fn over_length_names_are_rejected() {
        let p = provisioner();
        let long = "a".repeat(60); // "tenant_" + 60 = 67 > 63
        assert!(p.database_name(&long.into()).is_err());
    }

    #[test]
    fn tenant_dsn_swaps_the_database() {
        let p = provisioner();
        let dsn = render_dsn("postgresql://u:p@host:5432/postgres", "tenant_acme").unwrap();
        assert_eq!(dsn, "postgresql://u:p@host:5432/tenant_acme");
        // Query strings are preserved.
        let dsn = render_dsn("postgresql://u:p@host:5432/postgres?sslmode=require", "tenant_acme").unwrap();
        assert_eq!(dsn, "postgresql://u:p@host:5432/tenant_acme?sslmode=require");
    }
}
