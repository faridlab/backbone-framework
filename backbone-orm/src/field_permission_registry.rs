//! The permission-gated fields of every entity this process serves through the generic routes,
//! by table.
//!
//! A related row that `?include=` hydrates is read raw, so a permission gate declared on the
//! target entity must be findable from the table name the relation names — a cross-module
//! `@include` knows nothing else about its target. Each generic route builder registers its
//! entity's [`field_permissions`](crate::generic_repository::EntityRepoMeta::field_permissions)
//! here under its table, and the expansion redacts the fields whose permission the caller does
//! not hold. A table whose module never builds a generic route registers nothing.
//!
//! Keys are the bare table name, lower-cased (`hr.employees` and `employees` are one key); two
//! tables of the same name in different schemas share the union of their gated fields, which can
//! only redact more.
//!
//! Unlike the secret registry (which the source module also knows through
//! `relation_secret_fields`), there is deliberately no per-reaction spelling of this on the
//! including side: the permission a field needs belongs to the entity that owns the field, in
//! its own generated code, and nowhere else.

use std::collections::HashMap;
use std::sync::{OnceLock, RwLock};

fn registry() -> &'static RwLock<HashMap<String, Vec<(&'static str, &'static str)>>> {
    static REGISTRY: OnceLock<RwLock<HashMap<String, Vec<(&'static str, &'static str)>>>> =
        OnceLock::new();
    REGISTRY.get_or_init(|| RwLock::new(HashMap::new()))
}

fn key(table: &str) -> String {
    table
        .rsplit('.')
        .next()
        .unwrap_or(table)
        .trim_matches('"')
        .to_ascii_lowercase()
}

/// Record `fields` (response keys, camelCase, each with the permission that unlocks it) as the
/// permission-gated fields of `table`.
pub fn register(table: &str, fields: &'static [(&'static str, &'static str)]) {
    if fields.is_empty() {
        return;
    }
    let mut map = registry().write().unwrap_or_else(|e| e.into_inner());
    let entry = map.entry(key(table)).or_default();
    for field in fields {
        if !entry.iter().any(|(f, _)| *f == field.0) {
            entry.push(*field);
        }
    }
}

/// The permission-gated fields registered for `table` (empty when none were).
pub fn field_permissions_of(table: &str) -> Vec<(&'static str, &'static str)> {
    let map = registry().read().unwrap_or_else(|e| e.into_inner());
    map.get(&key(table)).cloned().unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_table_registered_in_any_spelling_answers_its_gated_fields() {
        register(
            "registry_probe.payroll",
            &[("baseSalary", "payroll_entry:read_salary")],
        );
        register("PAYROLL_OTHER", &[]);
        assert_eq!(
            field_permissions_of("payroll"),
            vec![("baseSalary", "payroll_entry:read_salary")]
        );
        assert_eq!(
            field_permissions_of("\"hr\".\"payroll\""),
            vec![("baseSalary", "payroll_entry:read_salary")]
        );
        assert!(field_permissions_of("payroll_other").is_empty());
        assert!(field_permissions_of("never_registered").is_empty());
    }

    #[test]
    fn a_field_registered_twice_answers_once() {
        register(
            "registry_dupe.accounts",
            &[("bidValue", "account:read_bid")],
        );
        register("other.accounts", &[("bidValue", "account:read_bid")]);
        assert_eq!(
            field_permissions_of("accounts"),
            vec![("bidValue", "account:read_bid")]
        );
    }
}
