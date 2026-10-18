//! The shape of a refused write: one [`Violation`] per broken rule, each with
//! the field it concerns, a stable code, the values the rule was checked
//! against, and a sentence a person can act on.
//!
//! ```json
//! { "success": false,
//!   "error": "validation failed: field_not_writable: `status` …",
//!   "violations": [
//!     { "path": "status", "code": "field_not_writable", "params": {}, "message": "…" } ] }
//! ```
//!
//! The `error` string stays, so a client that reads only it keeps working; a
//! form reads `violations` to mark each field by its code. A violation the
//! database raises carries the same code as the application-side check, read
//! from the constraint's name (see [`code_from_constraint`]).

use serde::{Deserialize, Serialize};

/// The code of a violation that refuses one query parameter of a list-shaped request.
pub const INVALID_QUERY: &str = "invalid_query";

/// One broken rule.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct Violation {
    /// The field it concerns, in the casing the entity serializes, with list
    /// indices for child rows (`lines/3/amount`); empty for a whole-record rule.
    pub path: String,
    /// A stable snake_case code, the same on every layer that enforces it.
    pub code: String,
    /// The values the rule was checked against (`{ "max": 255 }`).
    #[serde(default, skip_serializing_if = "serde_json::Map::is_empty")]
    pub params: serde_json::Map<String, serde_json::Value>,
    /// What went wrong, said so a person can fix it.
    pub message: String,
}

impl Violation {
    pub fn new(path: impl Into<String>, code: impl Into<String>, message: impl Into<String>) -> Self {
        Self { path: path.into(), code: code.into(), params: serde_json::Map::new(), message: message.into() }
    }

    /// A refused query parameter: the code [`INVALID_QUERY`] on the parameter's path, with the
    /// parameter and the stable `reason` it was refused for in `params`.
    pub fn invalid_query(parameter: &str, reason: &str, message: impl Into<String>) -> Self {
        Self::new(parameter, INVALID_QUERY, message)
            .with_param("parameter", parameter)
            .with_param("reason", reason)
    }

    /// Add one checked-against value.
    pub fn with_param(mut self, name: impl Into<String>, value: impl Into<serde_json::Value>) -> Self {
        self.params.insert(name.into(), value.into());
        self
    }
}

/// The path of a violation on item `index` of a batch: the index becomes the first segment, so
/// `name` on the fourth item reads `3/name` and a whole-record rule reads `3`.
pub fn batch_item_path(index: usize, path: &str) -> String {
    if path.is_empty() {
        index.to_string()
    } else {
        format!("{index}/{path}")
    }
}

/// The rule code a database constraint stands for.
///
/// Generated constraints are named `ck_<table>__<code>` (CHECK),
/// `uq_<table>__<code>` (unique index), `ex_<table>__<code>` (exclusion) and
/// `tg_<table>__<code>` (constraint trigger), so a violation the database
/// raises maps back to the code of the rule it enforces. A constraint named
/// otherwise is its own code: hand-written triggers raise with a constraint
/// name chosen to be read (`fiscal_period_status_transition`).
pub fn code_from_constraint(constraint: &str) -> String {
    for prefix in ["ck_", "uq_", "ex_", "tg_"] {
        if let Some(rest) = constraint.strip_prefix(prefix) {
            if let Some((_, code)) = rest.split_once("__") {
                if !code.is_empty() {
                    return code.to_string();
                }
            }
        }
    }
    constraint.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_generated_constraint_name_carries_its_rule_code() {
        assert_eq!(code_from_constraint("ck_journals__unbalanced"), "unbalanced");
        assert_eq!(code_from_constraint("uq_accounts__account_code_taken"), "account_code_taken");
        assert_eq!(code_from_constraint("ex_tax_rates__overlapping_validity"), "overlapping_validity");
        // A constraint named for reading is its own code.
        assert_eq!(code_from_constraint("fiscal_period_status_transition"), "fiscal_period_status_transition");
        // A prefix without the separator is not a generated name.
        assert_eq!(code_from_constraint("ck_legacy"), "ck_legacy");
    }

    #[test]
    fn a_refused_query_parameter_names_itself_and_its_reason() {
        let v = Violation::invalid_query("sort_by", "unknown_column", "`sort_by`: `x` is not a field");
        assert_eq!(
            serde_json::to_value(&v).unwrap(),
            serde_json::json!({
                "path": "sort_by",
                "code": "invalid_query",
                "params": { "parameter": "sort_by", "reason": "unknown_column" },
                "message": "`sort_by`: `x` is not a field"
            })
        );
    }

    #[test]
    fn a_violation_serializes_without_empty_params() {
        let v = Violation::new("status", "field_not_writable", "`status` changes only through its verbs");
        assert_eq!(
            serde_json::to_value(&v).unwrap(),
            serde_json::json!({ "path": "status", "code": "field_not_writable", "message": "`status` changes only through its verbs" })
        );
        let v = Violation::new("name", "max_length", "too long").with_param("max", 255);
        assert_eq!(serde_json::to_value(&v).unwrap()["params"], serde_json::json!({ "max": 255 }));
    }
}
