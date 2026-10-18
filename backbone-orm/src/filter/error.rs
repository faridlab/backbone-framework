//! The refusal of one query parameter, named.
//!
//! A list request carries its question in the query string: filters, sorts, paging,
//! aggregates. When one of them cannot be applied, the caller needs to know WHICH one and
//! why, in terms of what they sent — not the database's sentence, which carries SQL and the
//! names of tables and types. [`QueryParamError`] is that answer: the parameter as the client
//! spelled it, a stable reason code, and a sentence that never names an internal identifier.
//!
//! The error travels as an ordinary `anyhow` error out of the repository; `backbone-core`
//! turns it into an `invalid_query` violation on the parameter's path.

use std::collections::HashMap;

use super::parser::{is_internal_key, to_snake_case};

/// Why a query parameter was refused. The code ([`as_str`](Self::as_str)) is the stable,
/// snake_case `reason` clients branch on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum QueryParamReason {
    /// A column the resource does not have (a filter, a sort or an aggregate names it).
    UnknownColumn,
    /// A `field[operator]` filter whose operator is not one of the filter grammar's.
    UnknownOperator,
    /// A parameter that names a field with characters no column carries.
    InvalidFieldName,
    /// A value the named column cannot take (not a number, not a UUID, not a date, not one
    /// of an enum's values, out of range).
    InvalidValue,
    /// A paging parameter that is not a whole, non-negative number.
    NotANumber,
    /// A paging position (`after`, `before`) that this endpoint did not issue for this sort.
    InvalidCursor,
    /// A page beyond the deepest an offset walk may reach.
    TooDeep,
    /// An `include`/`with` name that is not a relation of the resource.
    UnknownRelation,
    /// A column the reduction cannot be taken over (`sum` of a text column).
    NotNumeric,
    /// An operator the named column's type does not support (`contain` on a number).
    UnsupportedOperator,
}

impl QueryParamReason {
    /// The stable code, as it appears in a violation's `params.reason`.
    pub fn as_str(self) -> &'static str {
        match self {
            QueryParamReason::UnknownColumn => "unknown_column",
            QueryParamReason::UnknownOperator => "unknown_operator",
            QueryParamReason::InvalidFieldName => "invalid_field_name",
            QueryParamReason::InvalidValue => "invalid_value",
            QueryParamReason::NotANumber => "not_a_number",
            QueryParamReason::InvalidCursor => "invalid_cursor",
            QueryParamReason::TooDeep => "too_deep",
            QueryParamReason::UnknownRelation => "unknown_relation",
            QueryParamReason::NotNumeric => "not_numeric",
            QueryParamReason::UnsupportedOperator => "unsupported_operator",
        }
    }
}

/// One query parameter that could not be applied.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("{message}")]
pub struct QueryParamError {
    /// The parameter as the client sent it (`sort_by`, `priority[foo]`, `include`).
    pub parameter: String,
    /// Why it was refused.
    pub reason: QueryParamReason,
    /// A sentence naming the parameter and the reason, safe to send to the client.
    pub message: String,
}

impl QueryParamError {
    /// A refusal of `parameter` for `reason`, said by `message`.
    pub fn new(
        parameter: impl Into<String>,
        reason: QueryParamReason,
        message: impl Into<String>,
    ) -> Self {
        Self {
            parameter: parameter.into(),
            reason,
            message: message.into(),
        }
    }

    /// `parameter` names a column the resource does not have.
    pub fn unknown_column(parameter: &str, column: &str) -> Self {
        Self::new(
            parameter,
            QueryParamReason::UnknownColumn,
            format!("`{parameter}`: `{column}` is not a field of this resource"),
        )
    }

    /// `parameter` is not a whole, non-negative number.
    pub fn not_a_number(parameter: &str, value: &str) -> Self {
        Self::new(
            parameter,
            QueryParamReason::NotANumber,
            format!(
                "`{parameter}` must be a whole number of zero or more, not `{}`",
                clip(value)
            ),
        )
    }
}

/// A client's value, cut so a very long one cannot flood the answer.
fn clip(value: &str) -> String {
    const MAX: usize = 40;
    if value.chars().count() <= MAX {
        value.to_string()
    } else {
        format!("{}…", value.chars().take(MAX).collect::<String>())
    }
}

/// Query keys that are grammar, not columns: they never name a column in a refusal.
const GRAMMAR_KEYS: [&str; 17] = [
    "orderby",
    "sort",
    "search",
    "searchfields",
    "limit",
    "offset",
    "page",
    "pagesize",
    "perpage",
    "per_page",
    "after",
    "before",
    "estimate",
    "estimate_total",
    "fields",
    "include",
    "with",
];

fn is_grammar_key(base: &str) -> bool {
    GRAMMAR_KEYS.contains(&base.to_ascii_lowercase().as_str())
}

/// Split `a[b]` into (`a`, Some(`b`)); a key without brackets is (key, None).
fn split_key(key: &str) -> (&str, Option<&str>) {
    match key.find('[') {
        Some(open) if key.ends_with(']') => (&key[..open], Some(&key[open + 1..key.len() - 1])),
        _ => (key, None),
    }
}

/// The sort columns an `orderby`/`sort` value names (`name,-age`, `age:desc`).
fn sort_columns(value: &str) -> impl Iterator<Item = String> + '_ {
    value.split(',').filter_map(|part| {
        let part = part.trim().trim_start_matches('-');
        let part = part.split(':').next().unwrap_or(part).trim();
        (!part.is_empty()).then(|| to_snake_case(part))
    })
}

/// The query parameters that name `column` (a snake_case column), in a stable order: a plain
/// filter key, a bracket filter's field, a sort's column, a search column list.
fn parameters_naming_column(filters: &HashMap<String, String>, column: &str) -> Vec<String> {
    let mut found: Vec<String> = Vec::new();
    for (key, value) in filters {
        if is_internal_key(key) {
            continue;
        }
        let (base, slot) = split_key(key);
        let lower = base.to_ascii_lowercase();
        let hit = if lower == "orderby" || lower == "sort" {
            match slot {
                Some(col) => to_snake_case(col) == column,
                None => sort_columns(value).any(|c| c == column),
            }
        } else if lower == "searchfields" {
            value.split(',').any(|c| to_snake_case(c.trim()) == column)
        } else if is_grammar_key(base) {
            false
        } else {
            to_snake_case(base) == column
        };
        if hit {
            found.push(key.clone());
        }
    }
    found.sort();
    found
}

/// The filter parameters (not sorts or paging) whose value carries `literal`, whole or as
/// one element of a comma list, in a stable order.
fn parameters_carrying_value(filters: &HashMap<String, String>, literal: &str) -> Vec<String> {
    let mut found: Vec<String> = Vec::new();
    for (key, value) in filters {
        if is_internal_key(key) {
            continue;
        }
        let (base, _) = split_key(key);
        if is_grammar_key(base) {
            continue;
        }
        if value.trim() == literal || value.split(',').any(|v| v.trim() == literal) {
            found.push(key.clone());
        }
    }
    found.sort();
    found
}

/// The name Postgres quotes in `column "x" does not exist` (or `column t.x does not exist`),
/// without any table qualifier.
fn missing_column(message: &str) -> Option<String> {
    let rest = message.strip_prefix("column ")?;
    let name = rest.split(" does not exist").next()?;
    let name = name.trim().trim_matches('"');
    let name = name.rsplit('.').next().unwrap_or(name).trim_matches('"');
    (!name.is_empty()).then(|| name.to_string())
}

/// The value Postgres quotes after a type error: `... for type integer: "abc"`, `value "9999"
/// is out of range ...`.
fn quoted_literal(message: &str) -> Option<String> {
    if let Some(at) = message.find(": \"") {
        let rest = &message[at + 3..];
        let end = rest.rfind('"')?;
        return Some(rest[..end].to_string());
    }
    let start = message.find('"')? + 1;
    let end = message[start..].find('"')? + start;
    Some(message[start..end].to_string())
}

/// What a column of the type Postgres named takes, in words for a caller.
fn expected_of(message: &str) -> &'static str {
    let lower = message.to_ascii_lowercase();
    if lower.contains("for enum") {
        return "one of the accepted values";
    }
    let ty = lower
        .split(" for type ")
        .nth(1)
        .and_then(|t| t.split(':').next())
        .unwrap_or("")
        .trim();
    match ty {
        "integer" | "bigint" | "smallint" | "numeric" | "real" | "double precision" | "money" => {
            "a number"
        }
        "uuid" => "a UUID",
        "boolean" => "true or false",
        "date" => "a date",
        t if t.starts_with("timestamp") || t.starts_with("time") => "a date and time",
        _ if lower.contains("date/time") => "a date and time",
        _ => "a value of the column's type",
    }
}

/// The `field[operator]` parameters that could have raised an undefined-operator error: the
/// pattern operators (`contain`, `like`, …) when the database named `~~`, the others otherwise.
fn operator_candidates(filters: &HashMap<String, String>, pattern: bool) -> Vec<String> {
    let mut candidates: Vec<String> = filters
        .keys()
        .filter(|k| !is_internal_key(k))
        .filter_map(|k| {
            let (base, slot) = split_key(k);
            let op = slot?;
            if base.eq_ignore_ascii_case("orderby") || base.eq_ignore_ascii_case("sort") {
                return None;
            }
            let is_pattern = matches!(
                op.to_ascii_lowercase().as_str(),
                "like"
                    | "ilike"
                    | "notlike"
                    | "contain"
                    | "notcontain"
                    | "startwith"
                    | "startswith"
                    | "endwith"
                    | "endswith"
            );
            (is_pattern == pattern).then(|| k.clone())
        })
        .collect();
    candidates.sort();
    candidates
}

fn operator_refusal(parameter: String) -> QueryParamError {
    let (_, slot) = split_key(&parameter);
    let op = slot.unwrap_or("").to_string();
    QueryParamError::new(
        parameter.clone(),
        QueryParamReason::UnsupportedOperator,
        format!("`{parameter}`: the `{op}` operator does not apply to this field's type"),
    )
}

/// Whether `err` is the database's refusal of an operator its operand types lack, and whether
/// it was a pattern operator (`~~`).
pub fn undefined_operator(err: &anyhow::Error) -> Option<bool> {
    match err.downcast_ref::<sqlx::Error>() {
        Some(sqlx::Error::Database(db)) if db.code().as_deref() == Some("42883") => {
            Some(db.message().contains("~~"))
        }
        _ => None,
    }
}

/// [`attribute_query_error`] for an undefined-operator refusal that more than one filter could
/// have raised: `column_types` (column name to Postgres `data_type`, read from the catalog)
/// narrows the candidates to the filters on columns the operator cannot apply to — a pattern
/// operator on anything but text. Left unchanged when that is still not exactly one.
pub fn attribute_operator_error(
    err: anyhow::Error,
    filters: &HashMap<String, String>,
    column_types: &HashMap<String, String>,
) -> anyhow::Error {
    let Some(pattern) = undefined_operator(&err) else {
        return err;
    };
    let mut narrowed: Vec<String> = operator_candidates(filters, pattern)
        .into_iter()
        .filter(|key| {
            let (base, _) = split_key(key);
            let ty = column_types
                .get(&to_snake_case(base))
                .map(|t| t.to_ascii_lowercase());
            match ty {
                Some(t) if pattern => !matches!(
                    t.as_str(),
                    "text" | "character varying" | "character" | "citext"
                ),
                _ => true,
            }
        })
        .collect();
    if narrowed.len() == 1 {
        operator_refusal(narrowed.remove(0)).into()
    } else {
        err
    }
}

/// Name the query parameter behind a failed list-shaped statement.
///
/// The database refuses a bad column or value with its own sentence (SQL, table and type
/// names), which the HTTP layer never sends. When the failure is one the request caused, this
/// finds the parameter in `filters` — the raw query map the statement was built from — and
/// returns the [`QueryParamError`] for it. Anything else, or a failure no parameter can be
/// matched to, comes back unchanged.
pub fn attribute_query_error(
    err: anyhow::Error,
    filters: &HashMap<String, String>,
) -> anyhow::Error {
    if err.downcast_ref::<QueryParamError>().is_some() {
        return err;
    }
    let Some(sqlx::Error::Database(db)) = err.downcast_ref::<sqlx::Error>() else {
        return err;
    };
    let code = db.code().map(|c| c.into_owned()).unwrap_or_default();
    let message = db.message().to_string();
    match code.as_str() {
        // undefined_column
        "42703" => {
            let Some(column) = missing_column(&message) else {
                return err;
            };
            match parameters_naming_column(filters, &column)
                .into_iter()
                .next()
            {
                Some(parameter) => QueryParamError::unknown_column(&parameter, &column).into(),
                None => err,
            }
        }
        // invalid_text_representation, numeric_value_out_of_range, invalid_datetime_format,
        // datetime_field_overflow, invalid_character_value_for_cast, character_not_in_repertoire
        "22P02" | "22003" | "22007" | "22008" | "22018" | "22021" => {
            let Some(literal) = quoted_literal(&message) else {
                return err;
            };
            match parameters_carrying_value(filters, &literal)
                .into_iter()
                .next()
            {
                Some(parameter) => {
                    let (base, _) = split_key(&parameter);
                    let field = base.to_string();
                    QueryParamError::new(
                        parameter.clone(),
                        QueryParamReason::InvalidValue,
                        format!(
                            "`{parameter}`: `{}` is not a value `{field}` can take; expected {}",
                            clip(&literal),
                            expected_of(&message)
                        ),
                    )
                    .into()
                }
                None => err,
            }
        }
        // undefined_function: an operator the column's type does not have (`contain` on a
        // number is `integer ~~ text`). Which parameter it was is known only when one filter
        // could have caused it; see [`attribute_operator_error`] for the rest.
        "42883" => {
            let mut candidates = operator_candidates(filters, message.contains("~~"));
            if candidates.len() == 1 {
                operator_refusal(candidates.remove(0)).into()
            } else {
                err
            }
        }
        _ => err,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn map(pairs: &[(&str, &str)]) -> HashMap<String, String> {
        pairs
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect()
    }

    #[test]
    fn a_missing_column_is_read_from_the_database_sentence() {
        assert_eq!(
            missing_column("column \"full_name\" does not exist").as_deref(),
            Some("full_name")
        );
        assert_eq!(
            missing_column("column t.full_name does not exist").as_deref(),
            Some("full_name")
        );
        assert_eq!(
            missing_column("column \"t\".\"full_name\" does not exist").as_deref(),
            Some("full_name")
        );
        assert_eq!(missing_column("relation \"x\" does not exist"), None);
    }

    #[test]
    fn a_quoted_literal_is_read_from_a_type_error() {
        assert_eq!(
            quoted_literal("invalid input syntax for type integer: \"abc\"").as_deref(),
            Some("abc")
        );
        assert_eq!(
            quoted_literal("invalid input value for enum project_kind: \"bogus\"").as_deref(),
            Some("bogus")
        );
        assert_eq!(
            quoted_literal("value \"99999999999\" is out of range for type integer").as_deref(),
            Some("99999999999")
        );
    }

    #[test]
    fn a_type_error_is_described_without_the_type_name_of_an_enum() {
        assert_eq!(
            expected_of("invalid input value for enum project_kind: \"x\""),
            "one of the accepted values"
        );
        assert_eq!(
            expected_of("invalid input syntax for type uuid: \"x\""),
            "a UUID"
        );
        assert_eq!(
            expected_of("invalid input syntax for type integer: \"x\""),
            "a number"
        );
        assert_eq!(
            expected_of("invalid input syntax for type boolean: \"x\""),
            "true or false"
        );
    }

    #[test]
    fn the_parameter_that_names_a_column_is_found_in_every_grammar() {
        let f = map(&[
            ("orderby", "-full_name,name:asc"),
            ("age[gte]", "3"),
            ("nick", "x"),
            ("limit", "3"),
        ]);
        assert_eq!(parameters_naming_column(&f, "full_name"), vec!["orderby"]);
        assert_eq!(parameters_naming_column(&f, "age"), vec!["age[gte]"]);
        assert_eq!(parameters_naming_column(&f, "nick"), vec!["nick"]);
        assert!(
            parameters_naming_column(&f, "limit").is_empty(),
            "grammar keys are not columns"
        );
        let g = map(&[
            ("orderby[createdAt]", "desc"),
            ("searchFields", "name, fullName"),
        ]);
        assert_eq!(
            parameters_naming_column(&g, "created_at"),
            vec!["orderby[createdAt]"]
        );
        assert_eq!(
            parameters_naming_column(&g, "full_name"),
            vec!["searchFields"]
        );
    }

    #[test]
    fn a_status_filter_is_a_column_not_grammar() {
        let f = map(&[("status", "open")]);
        assert_eq!(parameters_naming_column(&f, "status"), vec!["status"]);
    }

    #[test]
    fn the_parameter_carrying_a_value_is_found_whole_or_in_a_list() {
        let f = map(&[
            ("priority[in]", "1,abc,3"),
            ("name", "abc2"),
            ("orderby", "abc"),
        ]);
        assert_eq!(parameters_carrying_value(&f, "abc"), vec!["priority[in]"]);
    }

    #[test]
    fn the_candidates_for_an_operator_error_are_the_filters_of_its_family() {
        let f = map(&[
            ("name[startwith]", "S"),
            ("seq[contain]", "1"),
            ("seq[gte]", "1"),
            ("orderby[x]", "asc"),
        ]);
        assert_eq!(
            operator_candidates(&f, true),
            vec!["name[startwith]", "seq[contain]"]
        );
        assert_eq!(operator_candidates(&f, false), vec!["seq[gte]"]);
    }

    #[test]
    fn an_unrelated_error_is_left_alone() {
        let e = attribute_query_error(anyhow::anyhow!("pool closed"), &map(&[("a", "b")]));
        assert!(e.downcast_ref::<QueryParamError>().is_none());
        assert_eq!(e.to_string(), "pool closed");
    }

    #[test]
    fn a_refusal_already_named_passes_through() {
        let named = QueryParamError::unknown_column("sort_by", "x");
        let e = attribute_query_error(named.clone().into(), &map(&[]));
        assert_eq!(e.downcast_ref::<QueryParamError>(), Some(&named));
    }

    #[test]
    fn the_message_names_the_parameter_and_leaks_nothing_internal() {
        let e = QueryParamError::unknown_column("sort_by", "full_name");
        assert!(e.message.contains("sort_by") && e.message.contains("full_name"));
        assert_eq!(e.reason.as_str(), "unknown_column");
        let n = QueryParamError::not_a_number("offset", &"9".repeat(100));
        assert!(n.message.contains("offset"));
        assert!(
            n.message.chars().count() < 120,
            "a long value is cut: {}",
            n.message
        );
    }
}
