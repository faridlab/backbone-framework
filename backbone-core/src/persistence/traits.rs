//! Repository Traits - Contracts for Entity Persistence
//!
//! These traits define the contracts that entities and repositories must implement
//! to work with the generic repository implementations.

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::{de::DeserializeOwned, Serialize};
use std::collections::HashMap;
use std::fmt::Debug;

// ============================================================
// Error Types
// ============================================================

/// Repository error types
#[derive(Debug, thiserror::Error)]
pub enum RepositoryError {
    #[error("Entity not found")]
    NotFound,

    #[error("Entity already exists: {0}")]
    AlreadyExists(String),

    #[error("Validation error: {0}")]
    ValidationError(String),

    #[error("Database error: {0}")]
    DatabaseError(String),

    #[error("Serialization error: {0}")]
    SerializationError(String),

    #[error("Conflict: {0}")]
    Conflict(String),

    #[error("Internal error: {0}")]
    InternalError(String),

    /// The database refused the write through a constraint that stands for a
    /// rule: a foreign key, a CHECK, a generated unique index or an exclusion.
    /// One violation, coded as the application-side check is.
    #[error("validation failed: {}", .0.iter().map(|v| v.message.as_str()).collect::<Vec<_>>().join("; "))]
    Violations(Vec<crate::violation::Violation>),
}

impl RepositoryError {
    /// The repository error for a failure an inner layer reported as any error type.
    /// A database error inside it (however wrapped in context) converts as a `sqlx::Error`
    /// does, so a constraint the database refused with stays a violation; anything else
    /// keeps its sentence as a database error.
    pub fn from_source(e: impl Into<anyhow::Error>) -> Self {
        let e: anyhow::Error = e.into();
        #[cfg(feature = "postgres")]
        let e = match e.downcast::<sqlx::Error>() {
            Ok(db) => return RepositoryError::from(db),
            Err(e) => e,
        };
        RepositoryError::DatabaseError(e.to_string())
    }
}

impl From<serde_json::Error> for RepositoryError {
    fn from(e: serde_json::Error) -> Self {
        RepositoryError::SerializationError(e.to_string())
    }
}

#[cfg(feature = "postgres")]
impl From<sqlx::Error> for RepositoryError {
    fn from(e: sqlx::Error) -> Self {
        match e {
            sqlx::Error::RowNotFound => RepositoryError::NotFound,
            sqlx::Error::Database(db_err) => {
                let pg = db_err.try_downcast_ref::<sqlx::postgres::PgDatabaseError>();
                if let Some(v) = constraint_violation(
                    db_err.kind(),
                    db_err.code().as_deref(),
                    db_err.constraint(),
                    pg.and_then(|p| p.detail()).unwrap_or(""),
                    pg.and_then(|p| p.column()),
                    pg.and_then(|p| p.table()),
                ) {
                    return RepositoryError::Violations(vec![v]);
                }
                let msg = db_err.message().to_string();
                if msg.contains("duplicate key") || msg.contains("unique constraint") {
                    RepositoryError::AlreadyExists(msg)
                } else {
                    RepositoryError::DatabaseError(msg)
                }
            }
            _ => RepositoryError::DatabaseError(e.to_string()),
        }
    }
}

/// The violation a database constraint error stands for, or `None` for an error
/// that names no rule. A foreign key answers `reference_missing` on the column
/// that names a missing record, or `still_referenced` when a removed record is
/// still pointed at. A NOT NULL answers `required` on its column, worded as the
/// application-side required check is. A CHECK, a generated unique index (`uq_…`)
/// and an exclusion answer the code their constraint name carries (see
/// [`crate::violation::code_from_constraint`]). Another unique index keeps its
/// `AlreadyExists` answer.
///
/// The `code` stays machine-readable for clients to translate; the sentence is
/// for a person, so it never names a table, a constraint or a rule identifier.
/// `table` is the table Postgres reports, used to read the column out of a
/// generated CHECK's name, since Postgres reports no column for a CHECK.
#[cfg(feature = "postgres")]
fn constraint_violation(
    kind: sqlx::error::ErrorKind,
    sqlstate: Option<&str>,
    constraint: Option<&str>,
    detail: &str,
    column: Option<&str>,
    table: Option<&str>,
) -> Option<crate::violation::Violation> {
    use crate::violation::{code_from_constraint, Violation};
    use sqlx::error::ErrorKind;
    if kind == ErrorKind::NotNullViolation || sqlstate == Some("23502") {
        // 23502 names no constraint, only the column.
        let column = column.filter(|c| !c.is_empty())?;
        return Some(Violation::new(
            column,
            "required",
            format!("{column} is required"),
        ));
    }
    let constraint = constraint?;
    let code = code_from_constraint(constraint);
    let path = column
        .filter(|c| !c.is_empty())
        .map(str::to_string)
        .or_else(|| key_column(detail))
        .or_else(|| table.and_then(|t| check_column(constraint, t, &code)))
        .unwrap_or_default();
    Some(match kind {
        ErrorKind::ForeignKeyViolation if detail.contains("still referenced") => Violation::new(
            "",
            "still_referenced",
            "This record is still referenced by another record, so it cannot be removed",
        ),
        ErrorKind::ForeignKeyViolation => {
            let message = match path.as_str() {
                "" => "The record refers to a record that does not exist".to_string(),
                p => format!("{p} refers to a record that does not exist"),
            };
            Violation::new(path, "reference_missing", message)
        }
        ErrorKind::CheckViolation => {
            let message = check_sentence(&path, &code);
            Violation::new(path, code, message)
        }
        ErrorKind::UniqueViolation if constraint.starts_with("uq_") => {
            let message = match path.as_str() {
                "" => "This value is already taken by another record".to_string(),
                p => format!("{p} is already taken by another record"),
            };
            Violation::new(path, code, message)
        }
        _ if sqlstate == Some("23P01") => Violation::new(
            "",
            code,
            "The record overlaps another one it may not overlap",
        ),
        _ => return None,
    })
}

/// A person's sentence for a broken CHECK, keyed by its rule code. The generated
/// field checks' codes read as the application-side check does (without the
/// bound itself, which the constraint error does not carry); any other code
/// gets a neutral sentence rather than its identifier.
#[cfg(feature = "postgres")]
fn check_sentence(path: &str, code: &str) -> String {
    let field = if path.is_empty() {
        "A value".to_string()
    } else {
        path.to_string()
    };
    let predicate = match code {
        "required" => "is required",
        "max_length" => "is too long",
        "min_length" => "is too short",
        "length" => "does not have the required length",
        "max" => "is too large",
        "min" => "is too small",
        "range" => "is out of the allowed range",
        "positive" => "must be positive",
        "non_negative" => "must not be negative",
        "pattern" => "does not have the expected form",
        "alpha_dash" => "may hold only letters, digits, '_' and '-'",
        "alpha_num" => "may hold only letters and digits",
        "values" => "is not one of the allowed values",
        "email" => "must be an email address",
        "url" => "must be an absolute URL",
        "state_transition_not_declared" => "cannot move to that state from its current one",
        _ if path.is_empty() => return "The record does not satisfy one of its rules".to_string(),
        _ => "does not satisfy one of its rules",
    };
    format!("{field} {predicate}")
}

/// The column a generated field CHECK guards, read out of its name.
///
/// The schema generator names a field check `ck_<table>_<field>_<code>__<code>`
/// (e.g. `ck_tickets_title_max_length__max_length`); a row rule's name carries no
/// field and a name too long for Postgres ends in a hash, and neither yields one.
#[cfg(feature = "postgres")]
fn check_column(constraint: &str, table: &str, code: &str) -> Option<String> {
    let head = constraint.strip_prefix("ck_")?.split_once("__")?.0;
    let rest = head.strip_prefix(table)?.strip_prefix('_')?;
    let field = rest.strip_suffix(code)?.strip_suffix('_')?;
    (!field.is_empty()).then(|| field.to_string())
}

/// Columns a unique key carries for scoping, not as the value a person chose.
#[cfg(feature = "postgres")]
const SCOPE_COLUMNS: &[&str] = &[
    "company_id",
    "tenant_id",
    "org_unit_id",
    "organization_id",
    "deleted_at",
];

/// The column a constraint error's detail names (`Key (district_id)=(…)`). For a
/// composite key (`Key (company_id, code)=(…)`) it is the last column that is not
/// a tenant/scope column, read through an expression such as `lower(code::text)`.
#[cfg(feature = "postgres")]
fn key_column(detail: &str) -> Option<String> {
    let rest = detail.strip_prefix("Key (")?;
    let cols = &rest[..rest.find(")=")?];
    let names: Vec<String> = cols
        .split(',')
        .filter_map(|part| {
            // The first identifier inside an expression: `lower(sku::text)` → `sku`.
            let inner = part.rsplit('(').next().unwrap_or(part);
            let ident: String = inner
                .trim()
                .trim_matches('"')
                .chars()
                .take_while(|c| c.is_ascii_alphanumeric() || *c == '_')
                .collect();
            (!ident.is_empty()).then_some(ident)
        })
        .collect();
    names
        .iter()
        .rev()
        .find(|n| !SCOPE_COLUMNS.contains(&n.as_str()))
        .or_else(|| (names.len() == 1).then(|| &names[0]))
        .cloned()
}

#[cfg(all(test, feature = "postgres"))]
mod constraint_violation_tests {
    use super::*;
    use sqlx::error::ErrorKind;

    #[test]
    fn a_missing_reference_names_the_column_and_never_the_table() {
        let v = constraint_violation(
            ErrorKind::ForeignKeyViolation,
            Some("23503"),
            Some("fk_subdistricts_district_id"),
            "Key (district_id)=(1484f475-14be-4abc-985b-7291543d32bc) is not present in table \"districts\".",
            None,
            None,
        )
        .unwrap();
        assert_eq!((v.path.as_str(), v.code.as_str()), ("district_id", "reference_missing"));
        assert!(!v.message.contains("districts\"") && !v.message.contains("fk_"), "{}", v.message);
    }

    #[test]
    fn removing_a_referenced_record_is_still_referenced() {
        let v = constraint_violation(
            ErrorKind::ForeignKeyViolation,
            Some("23503"),
            Some("fk_provinces_country_id"),
            "Key (id)=(866f1b4e-a9f3-4626-95d1-13c183916b4b) is still referenced from table \"provinces\".",
            None,
            None,
        )
        .unwrap();
        assert_eq!((v.path.as_str(), v.code.as_str()), ("", "still_referenced"));
    }

    #[test]
    fn a_rule_constraint_answers_its_rules_code() {
        // Postgres reports no column for a CHECK; the generator's name carries it.
        let check = constraint_violation(ErrorKind::CheckViolation, Some("23514"), Some("ck_countries_name_max_length__max_length"), "", None, Some("countries")).unwrap();
        assert_eq!((check.path.as_str(), check.code.as_str()), ("name", "max_length"));
        let unique = constraint_violation(ErrorKind::UniqueViolation, Some("23505"), Some("uq_accounts__account_code_taken"), "Key (company_id, code)=(a, b) already exists.", None, Some("accounts")).unwrap();
        assert_eq!((unique.path.as_str(), unique.code.as_str()), ("code", "account_code_taken"));
        let overlap = constraint_violation(ErrorKind::Other, Some("23P01"), Some("ex_tax_rates__overlapping_validity"), "", None, Some("tax_rates")).unwrap();
        assert_eq!(overlap.code, "overlapping_validity");
    }

    #[test]
    fn a_missing_required_value_is_a_required_violation_on_its_column() {
        // 23502 names no constraint, only the column.
        let v = constraint_violation(
            ErrorKind::NotNullViolation,
            Some("23502"),
            None,
            "",
            Some("employee_id"),
            Some("payslips"),
        )
        .unwrap();
        assert_eq!(
            (v.path.as_str(), v.code.as_str()),
            ("employee_id", "required")
        );
        assert_eq!(v.message, "employee_id is required");
    }

    #[test]
    fn a_composite_unique_key_names_its_own_column_not_the_tenant() {
        let v = constraint_violation(
            ErrorKind::UniqueViolation,
            Some("23505"),
            Some("uq_accounts__code_taken"),
            "Key (company_id, code)=(7c0a…, 1100) already exists.",
            None,
            Some("accounts"),
        )
        .unwrap();
        assert_eq!(v.path, "code");
        assert!(
            v.message.starts_with("code "),
            "a sentence with a subject: {}",
            v.message
        );
        let scoped = constraint_violation(
            ErrorKind::UniqueViolation,
            Some("23505"),
            Some("uq_items__sku_taken"),
            "Key (org_unit_id, lower(sku::text))=(a, b) already exists.",
            None,
            Some("items"),
        )
        .unwrap();
        assert_eq!(scoped.path, "sku");
        let bare = constraint_violation(
            ErrorKind::UniqueViolation,
            Some("23505"),
            Some("uq_x__taken"),
            "",
            None,
            None,
        )
        .unwrap();
        assert_eq!(bare.path, "");
        assert!(
            bare.message.starts_with(char::is_uppercase),
            "{}",
            bare.message
        );
    }

    #[test]
    fn a_check_reads_as_a_sentence_never_as_the_rule_identifier() {
        let v = constraint_violation(
            ErrorKind::CheckViolation,
            Some("23514"),
            Some("ck_tickets_title_max_length__max_length"),
            "",
            None,
            Some("tickets"),
        )
        .unwrap();
        assert_eq!(v.code, "max_length", "the code stays machine-readable");
        assert!(
            !v.message.contains('`') && !v.message.contains("max_length"),
            "{}",
            v.message
        );
        assert!(v.message.starts_with("title "), "{}", v.message);
        // A row rule names no single column: a whole-record sentence, still without identifiers.
        let row = constraint_violation(
            ErrorKind::CheckViolation,
            Some("23514"),
            Some("ck_journals_g_jrn_1__unbalanced"),
            "",
            None,
            Some("journals"),
        )
        .unwrap();
        assert_eq!((row.path.as_str(), row.code.as_str()), ("", "unbalanced"));
        assert!(
            !row.message.contains("unbalanced") && !row.message.contains('`'),
            "{}",
            row.message
        );
        // A hand-written trigger sets the column itself.
        let tg = constraint_violation(
            ErrorKind::CheckViolation,
            Some("23514"),
            Some("tg_journals__state_transition_not_declared"),
            "",
            Some("status"),
            Some("journals"),
        )
        .unwrap();
        assert_eq!(
            (tg.path.as_str(), tg.code.as_str()),
            ("status", "state_transition_not_declared")
        );
        assert!(
            !tg.message.contains("state_transition_not_declared"),
            "{}",
            tg.message
        );
    }

    #[test]
    fn a_database_error_inside_any_error_converts_as_the_database_error_does() {
        let wrapped = anyhow::Error::new(sqlx::Error::RowNotFound).context("loading the row");
        assert!(matches!(RepositoryError::from_source(wrapped), RepositoryError::NotFound));
        assert!(matches!(RepositoryError::from_source(sqlx::Error::RowNotFound), RepositoryError::NotFound));
        let other = RepositoryError::from_source(anyhow::anyhow!("pool closed"));
        assert!(matches!(other, RepositoryError::DatabaseError(ref m) if m == "pool closed"), "{other:?}");
    }

    #[test]
    fn an_error_that_names_no_rule_keeps_its_old_answer() {
        assert!(constraint_violation(
            ErrorKind::UniqueViolation,
            Some("23505"),
            Some("users_email_key"),
            "",
            None,
            None
        )
        .is_none());
        assert!(
            constraint_violation(ErrorKind::Other, Some("42P01"), None, "", None, None).is_none()
        );
        // A not-null error without a column says nothing a person could act on.
        assert!(constraint_violation(
            ErrorKind::NotNullViolation,
            Some("23502"),
            None,
            "",
            None,
            None
        )
        .is_none());
    }
}

// ============================================================
// Entity Traits
// ============================================================

/// Trait for entities that can be persisted.
///
/// This trait defines the common fields and behavior required by all entities
/// that can be stored and retrieved from a repository.
pub trait PersistentEntity: Clone + Send + Sync + Debug + Serialize + DeserializeOwned + 'static {
    /// Get the entity's unique identifier
    fn entity_id(&self) -> String;

    /// Set the entity's unique identifier
    fn set_entity_id(&mut self, id: String);

    /// Get creation timestamp
    fn created_at(&self) -> Option<DateTime<Utc>>;

    /// Set creation timestamp
    fn set_created_at(&mut self, ts: DateTime<Utc>);

    /// Get last update timestamp
    fn updated_at(&self) -> Option<DateTime<Utc>>;

    /// Set last update timestamp
    fn set_updated_at(&mut self, ts: DateTime<Utc>);

    /// Get soft delete timestamp (None if not deleted)
    fn deleted_at(&self) -> Option<DateTime<Utc>>;

    /// Set soft delete timestamp
    fn set_deleted_at(&mut self, ts: Option<DateTime<Utc>>);

    /// Check if entity is soft-deleted
    fn is_deleted(&self) -> bool {
        self.deleted_at().is_some()
    }

    /// Mark entity as deleted (soft delete)
    fn mark_deleted(&mut self) {
        self.set_deleted_at(Some(Utc::now()));
        self.set_updated_at(Utc::now());
    }

    /// Restore a soft-deleted entity
    fn restore(&mut self) {
        self.set_deleted_at(None);
        self.set_updated_at(Utc::now());
    }

    /// Touch the entity (update timestamp)
    fn touch(&mut self) {
        self.set_updated_at(Utc::now());
    }

    /// Generate a new ID for this entity type
    fn generate_id() -> String {
        uuid::Uuid::new_v4().to_string()
    }

    /// Fields that a generic write (PUT, PATCH and their bulk forms) may not
    /// change, by their serialized name.
    ///
    /// The generic service refuses a write that would give one of these a new
    /// value, and still accepts a write that carries the stored value
    /// unchanged, so a form that sends the whole record keeps working. Code
    /// that owns the field — a verb, a write service — changes it through its
    /// own path. `id` and `metadata` are refused for every entity regardless
    /// of this list. The schema generator fills it from the fields the schema
    /// declares as not freely writable; the default protects nothing more.
    fn write_protected_fields() -> &'static [&'static str] {
        &[]
    }

    /// Fields a state machine owns, by their serialized name: they move only through the
    /// machine's verbs.
    ///
    /// A state field is usually also in [`Self::write_protected_fields`], which refuses it to
    /// every generic write. A verb's own write (one running inside
    /// [`as_transition`](crate::write_guard::as_transition)) may still move these fields — and
    /// only these: every other protected field stays refused inside a transition too. The
    /// schema generator fills it from the entity's state machines; the default is none, which
    /// leaves a protected state field refused to verbs as well.
    fn state_fields() -> &'static [&'static str] {
        &[]
    }
}

/// Trait for entities that support partial updates via field map
pub trait PartialUpdatable: PersistentEntity {
    /// Apply partial updates from a field map
    fn apply_partial_update(&mut self, fields: &HashMap<String, serde_json::Value>) -> Result<(), RepositoryError>;
}

/// Trait for entities with version/optimistic locking
pub trait Versioned {
    fn version(&self) -> u64;
    fn set_version(&mut self, version: u64);
    fn increment_version(&mut self) {
        self.set_version(self.version() + 1);
    }
}

// ============================================================
// Repository Traits
// ============================================================

/// Core CRUD repository trait
///
/// This trait defines the basic CRUD operations that all repositories must implement.
/// It's designed to work with the `CrudService` trait from the HTTP layer.
#[async_trait]
pub trait CrudRepository<E>: Send + Sync
where
    E: PersistentEntity,
{
    /// Create a new entity
    async fn create(&self, entity: E) -> Result<E, RepositoryError>;

    /// Find entity by ID (excluding soft-deleted)
    async fn find_by_id(&self, id: &str) -> Result<Option<E>, RepositoryError>;

    /// Find entity by ID (including soft-deleted, for trash operations)
    async fn find_by_id_including_deleted(&self, id: &str) -> Result<Option<E>, RepositoryError>;

    /// Update an existing entity
    async fn update(&self, entity: E) -> Result<E, RepositoryError>;

    /// Update an entity from `before` (the row as read) to `after`, writing only the fields
    /// whose value differs between the two. `Ok(None)` when the row no longer exists or was
    /// soft-deleted since it was read — the write must never revive it.
    ///
    /// The path every generic read-modify-write (PUT, PATCH) takes: a field the caller did not
    /// change keeps whatever value the database holds now, so a concurrent write to it (a verb
    /// moving a status) is not overwritten with the stale value read before it. The generated
    /// Postgres repositories override this with a changed-columns UPDATE guarded by the
    /// not-deleted predicate. The default falls back to the full [`Self::update`], which a
    /// repository with no partial write path has to accept.
    async fn update_changed(&self, before: &E, after: E) -> Result<Option<E>, RepositoryError> {
        let _ = before;
        match self.update(after).await {
            Ok(saved) => Ok(Some(saved)),
            Err(RepositoryError::NotFound) => Ok(None),
            Err(e) => Err(e),
        }
    }

    /// Soft delete an entity
    async fn soft_delete(&self, id: &str) -> Result<bool, RepositoryError>;

    /// Restore a soft-deleted entity
    async fn restore(&self, id: &str) -> Result<Option<E>, RepositoryError>;

    /// Permanently delete an entity
    async fn hard_delete(&self, id: &str) -> Result<bool, RepositoryError>;

    /// List entities with pagination (excluding soft-deleted)
    async fn list(&self, page: u32, limit: u32) -> Result<(Vec<E>, u64), RepositoryError>;

    /// List soft-deleted entities with pagination
    async fn list_deleted(&self, page: u32, limit: u32) -> Result<(Vec<E>, u64), RepositoryError>;

    /// Count all entities (excluding soft-deleted)
    async fn count(&self) -> Result<u64, RepositoryError>;

    /// Count soft-deleted entities
    async fn count_deleted(&self) -> Result<u64, RepositoryError>;

    /// Bulk create entities
    async fn bulk_create(&self, entities: Vec<E>) -> Result<Vec<E>, RepositoryError>;

    /// Permanently delete all soft-deleted entities
    async fn empty_trash(&self) -> Result<u64, RepositoryError>;

    /// List entities with pagination and filters (excluding soft-deleted)
    ///
    /// Default implementation ignores filters and delegates to `list()`.
    /// Override in repository implementations that support filter pushdown.
    async fn list_filtered(
        &self,
        page: u32,
        limit: u32,
        filters: HashMap<String, String>,
    ) -> Result<(Vec<E>, u64), RepositoryError> {
        let _ = filters; // ignored by default
        self.list(page, limit).await
    }

    /// `list_filtered`, carrying the pagination info (cursor positions
    /// included) so the HTTP layer can surface keyset paging. Default: the
    /// tuple form with the cursor fields absent; the generated Postgres
    /// repositories override this with the real keyset walk.
    async fn list_filtered_with_info(
        &self,
        page: u32,
        limit: u32,
        filters: HashMap<String, String>,
    ) -> Result<(Vec<E>, backbone_orm::repository::PaginationInfo), RepositoryError> {
        let (rows, total) = self.list_filtered(page, limit, filters).await?;
        Ok((
            rows,
            backbone_orm::repository::PaginationInfo::new(page, limit, total),
        ))
    }

    /// Group and reduce entities under the same filters as `list_filtered`.
    ///
    /// There is deliberately no useful default. A repository that cannot group
    /// must SAY so: an honest error tells the caller its chart is unavailable,
    /// whereas a default returning zeros would render an empty chart that looks
    /// like a real answer about a real, empty table. The Postgres-backed
    /// generated repositories override this via `impl_crud_repository!`.
    /// The schema-qualified table this repository reads, when it knows it.
    ///
    /// The Postgres-backed generated repositories override this; anything that
    /// cannot name a table returns `None` and simply has no history.
    fn table_name(&self) -> Option<&str> {
        None
    }

    async fn aggregate_filtered(
        &self,
        spec: &backbone_orm::repository::AggregateSpec,
        filters: HashMap<String, String>,
    ) -> Result<backbone_orm::repository::AggregateResult, RepositoryError> {
        let _ = (spec, filters);
        Err(RepositoryError::DatabaseError(
            "aggregate is not supported by this repository".to_string(),
        ))
    }

    /// Hydrate `?include=` relations: fetch rows from `table` by id list, as JSON
    /// (raw `row_to_json`). `table` comes from `EntityRepoMeta::relations()`
    /// (generator-emitted), never client input. Default: no expansion. The
    /// Postgres-backed generated repos override this via `impl_crud_repository!`.
    async fn fetch_related_json(
        &self,
        _table: &str,
        _ids: &[String],
    ) -> Vec<serde_json::Value> {
        Vec::new()
    }

    /// Check if an entity exists by ID
    async fn exists(&self, id: &str) -> Result<bool, RepositoryError> {
        Ok(self.find_by_id(id).await?.is_some())
    }

    // ── Atomic batch operations ───────────────────────────────────────────────
    //
    // The default implementations loop over the single-row methods and are NOT
    // transactional — they exist so non-Postgres implementors (mocks, custom
    // adapters) keep compiling. The macro-generated repositories override these
    // (via `impl_crud_repository!`) with truly atomic, single-transaction
    // versions backed by `GenericCrudRepository`.

    /// Soft-delete many entities by id. Returns the number affected.
    async fn bulk_soft_delete(&self, ids: &[String]) -> Result<u64, RepositoryError> {
        let mut n = 0;
        for id in ids {
            if self.soft_delete(id).await? {
                n += 1;
            }
        }
        Ok(n)
    }

    /// Restore many soft-deleted entities by id. Returns the restored entities.
    async fn bulk_restore(&self, ids: &[String]) -> Result<Vec<E>, RepositoryError> {
        let mut out = Vec::with_capacity(ids.len());
        for id in ids {
            if let Some(e) = self.restore(id).await? {
                out.push(e);
            }
        }
        Ok(out)
    }

    /// Permanently delete many entities by id. Returns the number affected.
    async fn bulk_hard_delete(&self, ids: &[String]) -> Result<u64, RepositoryError> {
        let mut n = 0;
        for id in ids {
            if self.hard_delete(id).await? {
                n += 1;
            }
        }
        Ok(n)
    }

    /// Restore every soft-deleted entity. Returns the restored entities.
    async fn restore_all(&self) -> Result<Vec<E>, RepositoryError> {
        let mut restored = Vec::new();
        // Restoring removes rows from the deleted set, so page 1 always returns
        // the next batch of still-deleted rows until none remain.
        loop {
            let (batch, _) = self.list_deleted(1, 500).await?;
            if batch.is_empty() {
                break;
            }
            for entity in &batch {
                if let Some(e) = self.restore(&entity.entity_id()).await? {
                    restored.push(e);
                }
            }
        }
        Ok(restored)
    }

    /// Update many entities atomically. Returns the updated entities.
    async fn bulk_update(&self, entities: Vec<E>) -> Result<Vec<E>, RepositoryError> {
        let mut out = Vec::with_capacity(entities.len());
        for entity in entities {
            out.push(self.update(entity).await?);
        }
        Ok(out)
    }
}

/// Extended repository with search/filter capabilities
#[async_trait]
pub trait SearchableRepository<E>: CrudRepository<E>
where
    E: PersistentEntity,
{
    /// Search entities with filters
    async fn search(
        &self,
        filters: HashMap<String, String>,
        page: u32,
        limit: u32,
    ) -> Result<(Vec<E>, u64), RepositoryError>;

    /// Find by a specific field value
    async fn find_by_field(&self, field: &str, value: &str) -> Result<Option<E>, RepositoryError>;

    /// Find all by a specific field value
    async fn find_all_by_field(
        &self,
        field: &str,
        value: &str,
        page: u32,
        limit: u32,
    ) -> Result<(Vec<E>, u64), RepositoryError>;
}

// ============================================================
// PostgreSQL-Specific Traits
// ============================================================

#[cfg(feature = "postgres")]
pub use postgres_traits::*;

#[cfg(feature = "postgres")]
mod postgres_traits {
    use super::*;
    use sqlx::postgres::PgRow;
    use sqlx::FromRow;

    /// Trait for mapping entities to/from PostgreSQL rows
    ///
    /// Implement this trait to enable automatic PostgreSQL persistence for your entity.
    ///
    /// # Example
    ///
    /// ```ignore
    /// use backbone_core::persistence::{PostgresEntity, PersistentEntity};
    /// use sqlx::FromRow;
    ///
    /// #[derive(Clone, Debug, Serialize, Deserialize, FromRow)]
    /// struct User {
    ///     id: String,
    ///     name: String,
    ///     email: String,
    ///     created_at: Option<DateTime<Utc>>,
    ///     updated_at: Option<DateTime<Utc>>,
    ///     deleted_at: Option<DateTime<Utc>>,
    /// }
    ///
    /// impl PostgresEntity for User {
    ///     fn table_name() -> &'static str { "users" }
    ///     fn select_columns() -> &'static [&'static str] {
    ///         &["id", "name", "email", "created_at", "updated_at", "deleted_at"]
    ///     }
    ///     fn insert_columns() -> &'static [&'static str] {
    ///         &["id", "name", "email", "created_at", "updated_at"]
    ///     }
    ///     fn update_columns() -> &'static [&'static str] {
    ///         &["name", "email", "updated_at"]
    ///     }
    ///     fn bind_for_insert(entity: &Self, query: Query<'_, ...>) -> Query<'_, ...> {
    ///         query.bind(&entity.id).bind(&entity.name).bind(&entity.email)
    ///             .bind(&entity.created_at).bind(&entity.updated_at)
    ///     }
    /// }
    /// ```
    pub trait PostgresEntity: PersistentEntity + for<'r> FromRow<'r, PgRow> + Unpin {
        /// Table name for this entity
        fn table_name() -> &'static str;

        /// Primary key column name (default: "id")
        fn id_column() -> &'static str {
            "id"
        }

        /// Column names for SELECT queries (excluding computed columns)
        fn select_columns() -> &'static [&'static str];

        /// Column names for INSERT (all columns that should be inserted)
        fn insert_columns() -> &'static [&'static str];

        /// Column names for UPDATE (columns that can be updated, excluding id)
        fn update_columns() -> &'static [&'static str] {
            Self::insert_columns()
        }

        /// Bind entity values to a query for INSERT
        ///
        /// The order must match insert_columns()
        fn bind_for_insert<'q>(
            entity: &'q Self,
            query: sqlx::query::Query<'q, sqlx::Postgres, sqlx::postgres::PgArguments>,
        ) -> sqlx::query::Query<'q, sqlx::Postgres, sqlx::postgres::PgArguments>;

        /// Bind entity values to a query for UPDATE
        ///
        /// The order must match update_columns()
        fn bind_for_update<'q>(
            entity: &'q Self,
            query: sqlx::query::Query<'q, sqlx::Postgres, sqlx::postgres::PgArguments>,
        ) -> sqlx::query::Query<'q, sqlx::Postgres, sqlx::postgres::PgArguments> {
            // Default: same as insert (override if different)
            Self::bind_for_insert(entity, query)
        }

        /// Build SELECT by ID query
        fn select_by_id_query() -> String {
            format!(
                "SELECT {} FROM {} WHERE {} = $1 AND deleted_at IS NULL",
                Self::select_columns().join(", "),
                Self::table_name(),
                Self::id_column()
            )
        }

        /// Build SELECT by ID including deleted
        fn select_by_id_including_deleted_query() -> String {
            format!(
                "SELECT {} FROM {} WHERE {} = $1",
                Self::select_columns().join(", "),
                Self::table_name(),
                Self::id_column()
            )
        }

        /// Build paginated list query
        fn list_query() -> String {
            format!(
                "SELECT {} FROM {} WHERE deleted_at IS NULL ORDER BY created_at DESC LIMIT $1 OFFSET $2",
                Self::select_columns().join(", "),
                Self::table_name()
            )
        }

        /// Build count query
        fn count_query() -> String {
            format!(
                "SELECT COUNT(*) FROM {} WHERE deleted_at IS NULL",
                Self::table_name()
            )
        }

        /// Build list deleted (trash) query
        fn list_deleted_query() -> String {
            format!(
                "SELECT {} FROM {} WHERE deleted_at IS NOT NULL ORDER BY deleted_at DESC LIMIT $1 OFFSET $2",
                Self::select_columns().join(", "),
                Self::table_name()
            )
        }

        /// Build count deleted query
        fn count_deleted_query() -> String {
            format!(
                "SELECT COUNT(*) FROM {} WHERE deleted_at IS NOT NULL",
                Self::table_name()
            )
        }

        /// Build soft delete query
        fn soft_delete_query() -> String {
            format!(
                "UPDATE {} SET deleted_at = NOW(), updated_at = NOW() WHERE {} = $1 AND deleted_at IS NULL",
                Self::table_name(),
                Self::id_column()
            )
        }

        /// Build restore query
        fn restore_query() -> String {
            format!(
                "UPDATE {} SET deleted_at = NULL, updated_at = NOW() WHERE {} = $1 AND deleted_at IS NOT NULL RETURNING {}",
                Self::table_name(),
                Self::id_column(),
                Self::select_columns().join(", ")
            )
        }

        /// Build hard delete query
        fn hard_delete_query() -> String {
            format!(
                "DELETE FROM {} WHERE {} = $1",
                Self::table_name(),
                Self::id_column()
            )
        }

        /// Build empty trash query
        fn empty_trash_query() -> String {
            format!(
                "DELETE FROM {} WHERE deleted_at IS NOT NULL",
                Self::table_name()
            )
        }
    }
}
