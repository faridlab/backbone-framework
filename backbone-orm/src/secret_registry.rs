//! The secret fields of every entity this process serves through the generic routes, by table.
//!
//! A related row that `?include=` hydrates is read raw, so it carries every column of its
//! table. When the related model sits in the same module the generator knows its secrets
//! (`EntityRepoMeta::relation_secret_fields`); a cross-module `@include` names only a table.
//! Each generic route builder registers its entity's secret fields here under its table, and
//! the expansion strips whatever a table registered. A table whose module never builds a
//! generic route registers nothing.
//!
//! Keys are the bare table name, lower-cased (`sapiens.users` and `users` are one key); two
//! tables of the same name in different schemas share the union of their secrets, which can
//! only strip more.

use std::collections::{HashMap, HashSet};
use std::sync::{OnceLock, RwLock};

fn registry() -> &'static RwLock<HashMap<String, HashSet<&'static str>>> {
    static REGISTRY: OnceLock<RwLock<HashMap<String, HashSet<&'static str>>>> = OnceLock::new();
    REGISTRY.get_or_init(|| RwLock::new(HashMap::new()))
}

fn key(table: &str) -> String {
    table.rsplit('.').next().unwrap_or(table).trim_matches('"').to_ascii_lowercase()
}

/// Record `fields` (response keys, camelCase) as the secrets of `table`.
pub fn register(table: &str, fields: &'static [&'static str]) {
    if fields.is_empty() {
        return;
    }
    let mut map = registry().write().unwrap_or_else(|e| e.into_inner());
    map.entry(key(table)).or_default().extend(fields.iter().copied());
}

/// The secrets registered for `table` (empty when none were).
pub fn secret_fields_of(table: &str) -> Vec<&'static str> {
    let map = registry().read().unwrap_or_else(|e| e.into_inner());
    map.get(&key(table)).map(|s| s.iter().copied().collect()).unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_table_registered_in_any_spelling_answers_its_secrets() {
        register("registry_probe.accounts", &["passwordHash"]);
        register("ACCOUNTS_OTHER", &[]);
        assert_eq!(secret_fields_of("accounts"), vec!["passwordHash"]);
        assert_eq!(secret_fields_of("\"registry_probe\".\"accounts\""), vec!["passwordHash"]);
        assert!(secret_fields_of("accounts_other").is_empty());
        assert!(secret_fields_of("never_registered").is_empty());
    }
}
