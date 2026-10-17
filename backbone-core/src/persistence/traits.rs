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

    /// [`from_source`](Self::from_source) for a batch write: when the failure is a
    /// [`backbone_orm::BatchItemError`], the error is placed on the item that failed (see
    /// [`at_batch_index`](Self::at_batch_index)).
    pub fn from_batch_source(e: impl Into<anyhow::Error>) -> Self {
        let e: anyhow::Error = e.into();
        match e.downcast::<backbone_orm::BatchItemError>() {
            Ok(item) => Self::from_source(item.error).at_batch_index(item.index),
            Err(e) => Self::from_source(e),
        }
    }

    /// Place this error on item `index` of a batch: a violation's path gains the index as its
    /// first segment (`3/name`, or `3` for a whole-record rule), and any other error's sentence
    /// names the item.
    pub fn at_batch_index(self, index: usize) -> Self {
        let item = |message: String| format!("item {index}: {message}");
        match self {
            RepositoryError::Violations(violations) => RepositoryError::Violations(
                violations
                    .into_iter()
                    .map(|mut v| {
                        v.path = crate::violation::batch_item_path(index, &v.path);
                        v
                    })
                    .collect(),
            ),
            RepositoryError::NotFound => RepositoryError::ValidationError(item(
                "the record no longer exists or was deleted".to_string(),
            )),
            RepositoryError::AlreadyExists(m) => RepositoryError::AlreadyExists(item(m)),
            RepositoryError::ValidationError(m) => RepositoryError::ValidationError(item(m)),
            RepositoryError::DatabaseError(m) => RepositoryError::DatabaseError(item(m)),
            RepositoryError::SerializationError(m) => RepositoryError::SerializationError(item(m)),
            RepositoryError::Conflict(m) => RepositoryError::Conflict(item(m)),
            RepositoryError::InternalError(m) => RepositoryError::InternalError(item(m)),
        }
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
                if let Some(v) = scope_refusal(db_err.code().as_deref(), db_err.message()) {
                    return RepositoryError::Violations(vec![v]);
                }
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
                    // Never the raw message: it names the index.
                    RepositoryError::Violations(vec![taken_violation("", "taken")])
                } else {
                    RepositoryError::DatabaseError(msg)
                }
            }
            _ => RepositoryError::DatabaseError(e.to_string()),
        }
    }
}

/// The violation a row-level security refusal stands for: a write whose new row the
/// table's policy does not admit (`42501` with the policy sentence), which is how a
/// forged `company_id` or `org_unit_id` is refused. It answers `scope_refused` with no
/// path, since Postgres names neither the policy's column nor the value, and a sentence
/// that names no table or policy. Any other `42501` (a missing grant) is a server fault
/// and stays one.
#[cfg(feature = "postgres")]
fn scope_refusal(sqlstate: Option<&str>, message: &str) -> Option<crate::violation::Violation> {
    (sqlstate == Some("42501") && message.contains("row-level security policy")).then(|| {
        crate::violation::Violation::new(
            "",
            "scope_refused",
            "The record is outside the company or unit this request may write to",
        )
    })
}

/// The `taken` answer for a unique key, on `path` when it is known.
#[cfg(feature = "postgres")]
fn taken_violation(path: &str, code: &str) -> crate::violation::Violation {
    let message = match path {
        "" => "This value is already taken by another record".to_string(),
        p => format!("{p} is already taken by another record"),
    };
    crate::violation::Violation::new(path, code, message)
}

/// The violation a database constraint error stands for, or `None` for an error
/// that names no rule. A foreign key answers `reference_missing` on the column
/// that names a missing record, or `still_referenced` when a removed record is
/// still pointed at. A NOT NULL answers `required` on its column, worded as the
/// application-side required check is. A CHECK and an exclusion answer the code
/// their constraint name carries (see [`crate::violation::code_from_constraint`]).
/// Every unique key answers too: the code its generated name carries
/// (`uq_…__<code>`), or `taken` for any other name, which is never repeated.
///
/// The `code` stays machine-readable for clients to translate; the sentence is
/// for a person, so it never names a table, a constraint or a rule identifier.
/// `table` is the table Postgres reports, used to read the column out of a
/// generated constraint's name (see [`constraint_column`]) when Postgres reports
/// none: a CHECK never carries one, and on a table with row-level security
/// Postgres leaves the `Key (…)` detail out of unique and foreign-key errors too.
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
    if kind == ErrorKind::UniqueViolation || sqlstate == Some("23505") {
        let generated = constraint.and_then(generated_code);
        let path = column
            .filter(|c| !c.is_empty())
            .map(str::to_string)
            .or_else(|| key_column(detail))
            .or_else(|| {
                let (c, t) = (constraint?, table?);
                constraint_column(c, t, generated.as_deref().unwrap_or(""))
            })
            .unwrap_or_default();
        return Some(taken_violation(&path, generated.as_deref().unwrap_or("taken")));
    }
    let constraint = constraint?;
    let code = code_from_constraint(constraint);
    let path = column
        .filter(|c| !c.is_empty())
        .map(str::to_string)
        .or_else(|| key_column(detail))
        .or_else(|| table.and_then(|t| constraint_column(constraint, t, &code)))
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

/// The code a generated constraint name carries after its `__`, or `None` for a name
/// the generator did not write (which must never reach a client as a code).
#[cfg(feature = "postgres")]
fn generated_code(constraint: &str) -> Option<String> {
    let code = crate::violation::code_from_constraint(constraint);
    (code != constraint).then_some(code)
}

/// Tenant keys a generated unique index names before its field, in key order.
#[cfg(feature = "postgres")]
const TENANT_KEYS: &[&str] = &["company_id", "tenant_id", "org_unit_id", "organization_id"];

/// The field a generated constraint concerns, read out of its name; `table` is the bare
/// table Postgres reports. The schema generator names a constraint
/// `<prefix>_<table>_<segment>__<code>` (`constraint_name` in metaphor-plugin-schema's
/// `generators/sql.rs`), and the segment names the field:
///
/// - a field check, `ck_<table>_<field>_<code>__<code>`
///   (`ck_tickets_title_max_length__max_length` → `title`);
/// - a row rule, `ck_<table>_<field>__<code>`, on the field its guard reports
///   (`ck_projects_kind__website_required_for_client` → `kind`);
/// - a unique index, `uq_<table>_[<tenant keys>_]<field>__<code>`
///   (`uq_projects_contact_email__unique` → `contact_email`,
///   `uq_projects_company_id_code__code_taken` → `code`);
/// - a foreign key, `fk_<table>_<column>` (or `fk_<table>_<column>_fk` on a link table).
///
/// Read back: the part between the prefix and the first `__`, less `<table>_`; a name too
/// long for Postgres, whose head the generator ends in `_<8 hex digits>`, names no field;
/// then a trailing `_<code>` is dropped, and leading tenant keys while something remains.
#[cfg(feature = "postgres")]
fn constraint_column(constraint: &str, table: &str, code: &str) -> Option<String> {
    let ident = |s: &str| {
        (!s.is_empty() && s.chars().all(|c| c.is_ascii_alphanumeric() || c == '_'))
            .then(|| s.to_string())
    };
    if let Some(rest) = constraint.strip_prefix("fk_") {
        let column = rest.strip_prefix(table)?.strip_prefix('_')?;
        return ident(column.strip_suffix("_fk").unwrap_or(column));
    }
    let head = ["ck_", "uq_", "ex_", "tg_"]
        .iter()
        .find_map(|p| constraint.strip_prefix(p))?
        .split_once("__")?
        .0;
    if ends_in_digest(head) {
        return None;
    }
    let mut rest = head.strip_prefix(table)?.strip_prefix('_')?;
    if !code.is_empty() {
        if let Some(field) = rest.strip_suffix(code).and_then(|f| f.strip_suffix('_')) {
            if !field.is_empty() {
                rest = field;
            }
        }
    }
    while let Some(next) = TENANT_KEYS.iter().find_map(|key| {
        rest.strip_prefix(key)?.strip_prefix('_').filter(|r| !r.is_empty())
    }) {
        rest = next;
    }
    ident(rest)
}

/// Whether a constraint name's head ends in the `_<8 hex digits>` digest the generator puts on
/// a name too long for Postgres.
#[cfg(feature = "postgres")]
fn ends_in_digest(head: &str) -> bool {
    let bytes = head.as_bytes();
    bytes.len() > 9
        && bytes[bytes.len() - 9] == b'_'
        && bytes[bytes.len() - 8..].iter().all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(b))
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
        // A rule whose name was shortened to a digest names no column: a whole-record
        // sentence, still without identifiers.
        let row = constraint_violation(
            ErrorKind::CheckViolation,
            Some("23514"),
            Some("ck_journals_lines_must_balance_1f2e3d4c__unbalanced"),
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
    fn every_unique_key_answers_taken_and_never_names_its_index() {
        // A name the generator did not write: `taken`, the path from the detail when given.
        let v = constraint_violation(ErrorKind::UniqueViolation, Some("23505"), Some("users_email_key"), "Key (email)=(a@b.c) already exists.", None, Some("users")).unwrap();
        assert_eq!((v.path.as_str(), v.code.as_str()), ("email", "taken"));
        assert!(!v.message.contains("users_email_key"), "{}", v.message);
        // RLS tables leave the detail out: a legacy `idx_` name yields no path, still coded.
        let v = constraint_violation(ErrorKind::UniqueViolation, Some("23505"), Some("idx_projects_contact_email"), "", None, Some("projects")).unwrap();
        assert_eq!((v.path.as_str(), v.code.as_str()), ("", "taken"));
        assert!(!v.message.contains("idx_"), "{}", v.message);
        // The generated name carries the path when the detail is absent: `@unique` …
        let v = constraint_violation(ErrorKind::UniqueViolation, Some("23505"), Some("uq_projects_contact_email__unique"), "", None, Some("projects")).unwrap();
        assert_eq!((v.path.as_str(), v.code.as_str()), ("contact_email", "unique"));
        assert!(!v.message.contains("uq_"), "{}", v.message);
        let v = constraint_violation(ErrorKind::UniqueViolation, Some("23505"), Some("uq_tickets_title__unique"), "", None, Some("tickets")).unwrap();
        assert_eq!((v.path.as_str(), v.code.as_str()), ("title", "unique"));
        // … a declared uniqueness rule, its tenant key dropped …
        let v = constraint_violation(ErrorKind::UniqueViolation, Some("23505"), Some("uq_projects_company_id_code__code_taken"), "", None, Some("projects")).unwrap();
        assert_eq!((v.path.as_str(), v.code.as_str()), ("code", "code_taken"));
        let v = constraint_violation(ErrorKind::UniqueViolation, Some("23505"), Some("uq_items_company_id_org_unit_id_sku__unique"), "", None, Some("items")).unwrap();
        assert_eq!(v.path, "sku");
        // … a field that is itself a tenant key stays when nothing else remains.
        let v = constraint_violation(ErrorKind::UniqueViolation, Some("23505"), Some("uq_members_org_unit_id__unique"), "", None, Some("members")).unwrap();
        assert_eq!(v.path, "org_unit_id");
        // A digest-shortened name names no field.
        let v = constraint_violation(ErrorKind::UniqueViolation, Some("23505"), Some("uq_projects_company_id_some_long_fiel_0a1b2c3d__code_taken"), "", None, Some("projects")).unwrap();
        assert_eq!((v.path.as_str(), v.code.as_str()), ("", "code_taken"));
        // The service-owned tenancy index, named without `__`: `taken`, never its name.
        let v = constraint_violation(ErrorKind::UniqueViolation, Some("23505"), Some("uq_tickets_org_unit_id_reference"), "", None, Some("tickets")).unwrap();
        assert_eq!((v.path.as_str(), v.code.as_str()), ("", "taken"));
        assert!(!v.message.contains("uq_"), "{}", v.message);
    }

    #[test]
    fn a_foreign_key_on_a_table_with_rls_names_its_column_from_the_constraint() {
        // Postgres omits `Key (…)` when the table has row-level security.
        let v = constraint_violation(ErrorKind::ForeignKeyViolation, Some("23503"), Some("fk_tickets_project_id"), "", None, Some("tickets")).unwrap();
        assert_eq!((v.path.as_str(), v.code.as_str()), ("project_id", "reference_missing"));
        assert!(!v.message.contains("fk_"), "{}", v.message);
        let link = constraint_violation(ErrorKind::ForeignKeyViolation, Some("23503"), Some("fk_user_roles_role_id_fk"), "", None, Some("user_roles")).unwrap();
        assert_eq!(link.path, "role_id");
        // Another table's key does not parse as this table's.
        let other = constraint_violation(ErrorKind::ForeignKeyViolation, Some("23503"), Some("fk_other_project_id"), "", None, Some("tickets")).unwrap();
        assert_eq!(other.path, "");
    }

    #[test]
    fn a_rule_check_about_a_field_answers_on_that_field() {
        let v = constraint_violation(ErrorKind::CheckViolation, Some("23514"), Some("ck_projects_kind__website_required_for_client"), "", None, Some("projects")).unwrap();
        assert_eq!((v.path.as_str(), v.code.as_str()), ("kind", "website_required_for_client"));
        assert!(!v.message.contains("ck_") && !v.message.contains("website_required"), "{}", v.message);
        // A field check keeps reading as one.
        let v = constraint_violation(ErrorKind::CheckViolation, Some("23514"), Some("ck_shifts_call_note_max_length__max_length"), "", None, Some("shifts")).unwrap();
        assert_eq!(v.path, "call_note");
        // A digest-shortened rule name keeps no path.
        let v = constraint_violation(ErrorKind::CheckViolation, Some("23514"), Some("ck_projects_a_very_long_field_name_dead00ff__website_required_for_client"), "", None, Some("projects")).unwrap();
        assert_eq!((v.path.as_str(), v.code.as_str()), ("", "website_required_for_client"));
    }

    #[test]
    fn a_row_security_refusal_is_scope_refused_and_a_missing_grant_is_not() {
        let v = scope_refusal(Some("42501"), "new row violates row-level security policy for table \"projects\"").unwrap();
        assert_eq!((v.path.as_str(), v.code.as_str()), ("", "scope_refused"));
        assert!(!v.message.contains("projects") && !v.message.contains("policy"), "{}", v.message);
        assert!(scope_refusal(Some("42501"), "permission denied for table projects").is_none());
        assert!(scope_refusal(Some("23505"), "row-level security policy").is_none());
    }

    #[test]
    fn an_error_that_names_no_rule_keeps_its_old_answer() {
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

/// A foreign key one of an entity's fields holds: `field` (the entity's serialized
/// field, which is also its column) names a row of `table` by its `column`.
///
/// The generic service checks, before a write that sets the field, that the row it
/// names is one the caller can see (see [`PersistentEntity::references`]): Postgres
/// checks a foreign key without row-level security, so without this a request scoped
/// to one company could point a record at another company's row.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Reference {
    /// The referencing field, by its serialized (snake_case) name; also the column.
    pub field: &'static str,
    /// The referenced table, bare (resolved in the referencing table's schema first)
    /// or schema-qualified.
    pub table: &'static str,
    /// The referenced column, usually `id`.
    pub column: &'static str,
}

impl Reference {
    /// A reference from `field` to `table`.`column`.
    pub const fn new(field: &'static str, table: &'static str, column: &'static str) -> Self {
        Self { field, table, column }
    }
}

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

    /// The foreign keys this entity's fields hold.
    ///
    /// Before a generic create, PUT, PATCH or bulk write stores a value in one of these
    /// fields (a create, or a write that changes it), the generic service asks the
    /// repository whether the row it names is visible to the caller, on the request's own
    /// connection and scope ([`CrudRepository::reference_visible`]). A row the caller
    /// cannot see — missing, or fenced off by row-level security — refuses the write with
    /// a `reference_missing` violation on the field, the same answer a missing row gets, so
    /// the refusal never tells that the row exists elsewhere. The schema generator fills it
    /// from the entity's relations as a `const` slice; the default checks nothing beyond
    /// the database's key.
    fn references() -> &'static [Reference] {
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

    /// List soft-deleted entities with pagination, filters and search — the trash twin of
    /// [`list_filtered`](Self::list_filtered).
    ///
    /// Default implementation ignores filters and delegates to `list_deleted()`. The generated
    /// repositories override it with the same filter pushdown the active list uses.
    async fn list_deleted_filtered(
        &self,
        page: u32,
        limit: u32,
        filters: HashMap<String, String>,
    ) -> Result<(Vec<E>, u64), RepositoryError> {
        let _ = filters;
        self.list_deleted(page, limit).await
    }

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

    /// The pool this repository writes through, when it has one.
    ///
    /// The generic service opens one write transaction on it around each write
    /// ([`backbone_orm::in_write_tx`]), so the write, its hooks and the events they stage commit
    /// or roll back together. `None` (the default: an in-memory repository, a mock) leaves the
    /// write without a transaction of its own. The Postgres-backed generated repositories
    /// override this via `impl_crud_repository!`.
    fn write_pool(&self) -> Option<&backbone_orm::PgPool> {
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

    /// Whether the row of `table` whose `column` holds `value` is visible to the caller:
    /// read on the request's own connection and scope, so a row row-level security hides
    /// counts as absent. `table` comes from [`PersistentEntity::references`]
    /// (generator-emitted), never from client input.
    ///
    /// `Ok(None)` means this repository cannot tell; the write is then left to the
    /// database's own key. That is the default. The Postgres-backed generated repositories
    /// override it via `impl_crud_repository!`.
    async fn reference_visible(
        &self,
        table: &str,
        column: &str,
        value: &serde_json::Value,
    ) -> Result<Option<bool>, RepositoryError> {
        let _ = (table, column, value);
        Ok(None)
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

    /// Update many entities from `(before, after)` pairs, writing for each only the fields that
    /// changed — the batch twin of [`update_changed`](Self::update_changed), and the path the
    /// generic bulk PUT and bulk PATCH take. A row missing or deleted since it was read fails
    /// the batch, with the error placed on that item.
    ///
    /// The generated Postgres repositories override this with one transaction. The default
    /// loops over `update_changed` and is not transactional.
    async fn bulk_update_changed(&self, pairs: Vec<(E, E)>) -> Result<Vec<E>, RepositoryError> {
        let mut out = Vec::with_capacity(pairs.len());
        for (index, (before, after)) in pairs.into_iter().enumerate() {
            match self.update_changed(&before, after).await {
                Ok(Some(saved)) => out.push(saved),
                Ok(None) => return Err(RepositoryError::NotFound.at_batch_index(index)),
                Err(e) => return Err(e.at_batch_index(index)),
            }
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
