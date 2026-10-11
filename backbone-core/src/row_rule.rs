//! The values a compiled row rule computes with, and its operators.
//!
//! The schema generator compiles each row rule a module declares (a condition
//! over one row's own fields) into a Rust expression over [`Val`], built from
//! the functions here, and evaluates it against the row the write would store,
//! read through the row's JSON form so enums, decimals and dates arrive as the
//! text and numbers their columns hold.
//!
//! The semantics are SQL's, because the same rule also compiles to a `CHECK`
//! constraint and both must give one answer:
//!
//! - A missing or null field is [`Val::Null`], and an operator over a null is
//!   null: `name.length >= 1` with no name is neither true nor false.
//! - `&&`, `||` and `!` are three-valued: `false && null` is false,
//!   `true || null` is true, anything else with a null is null.
//! - A rule [`holds`] unless it is definitely false, as a `CHECK` passes on
//!   null. `x == null` and `x != null` test for null itself and are never null.
//! - A comparison between a number and a text parses the text, the way a
//!   decimal column serialized as `"12.50"` compares to `0`.

use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};

use serde_json::Value;

/// One value inside a rule.
#[derive(Debug, Clone, PartialEq)]
pub enum Val {
    Null,
    Bool(bool),
    Num(f64),
    Text(String),
    List(Vec<Val>),
}

impl Val {
    fn from_json(v: &Value) -> Val {
        match v {
            Value::Null | Value::Object(_) => Val::Null,
            Value::Bool(b) => Val::Bool(*b),
            Value::Number(n) => n.as_f64().map(Val::Num).unwrap_or(Val::Null),
            Value::String(s) => Val::Text(s.clone()),
            Value::Array(items) => Val::List(items.iter().map(Val::from_json).collect()),
        }
    }

    fn num(&self) -> Option<f64> {
        match self {
            Val::Num(n) => Some(*n),
            Val::Text(s) => s.trim().parse().ok(),
            _ => None,
        }
    }

    fn text(&self) -> Option<&str> {
        match self {
            Val::Text(s) => Some(s),
            _ => None,
        }
    }

    fn truth(&self) -> Option<bool> {
        match self {
            Val::Bool(b) => Some(*b),
            _ => None,
        }
    }
}

/// The row's field `name`; [`Val::Null`] when it is absent or null.
pub fn field(row: &Value, name: &str) -> Val {
    row.get(name).map(Val::from_json).unwrap_or(Val::Null)
}

pub fn text(s: &str) -> Val {
    Val::Text(s.to_string())
}

/// Whether a rule's value lets the write through: everything but a definite `false`.
pub fn holds(v: &Val) -> bool {
    !matches!(v, Val::Bool(false))
}

// ── Null tests ───────────────────────────────────────────────────────────

pub fn is_null(a: Val) -> Val {
    Val::Bool(matches!(a, Val::Null))
}

pub fn is_not_null(a: Val) -> Val {
    Val::Bool(!matches!(a, Val::Null))
}

// ── Logic ────────────────────────────────────────────────────────────────

pub fn and(a: Val, b: Val) -> Val {
    match (a.truth(), b.truth()) {
        (Some(false), _) | (_, Some(false)) => Val::Bool(false),
        (Some(true), Some(true)) => Val::Bool(true),
        _ => Val::Null,
    }
}

pub fn or(a: Val, b: Val) -> Val {
    match (a.truth(), b.truth()) {
        (Some(true), _) | (_, Some(true)) => Val::Bool(true),
        (Some(false), Some(false)) => Val::Bool(false),
        _ => Val::Null,
    }
}

pub fn not(a: Val) -> Val {
    a.truth().map(|b| Val::Bool(!b)).unwrap_or(Val::Null)
}

/// `c ? t : e`, as SQL's `CASE WHEN c THEN t ELSE e END`: a null condition takes `e`.
pub fn cond(c: Val, t: Val, e: Val) -> Val {
    if c.truth() == Some(true) {
        t
    } else {
        e
    }
}

// ── Comparison ───────────────────────────────────────────────────────────

fn order(a: &Val, b: &Val) -> Option<std::cmp::Ordering> {
    match (a, b) {
        (Val::Null, _) | (_, Val::Null) => None,
        (Val::Text(x), Val::Text(y)) => Some(x.cmp(y)),
        (Val::Bool(x), Val::Bool(y)) => Some(x.cmp(y)),
        _ => a.num()?.partial_cmp(&b.num()?),
    }
}

fn compare(a: Val, b: Val, test: fn(std::cmp::Ordering) -> bool) -> Val {
    order(&a, &b).map(|o| Val::Bool(test(o))).unwrap_or(Val::Null)
}

pub fn eq(a: Val, b: Val) -> Val {
    compare(a, b, |o| o.is_eq())
}

pub fn ne(a: Val, b: Val) -> Val {
    compare(a, b, |o| o.is_ne())
}

pub fn lt(a: Val, b: Val) -> Val {
    compare(a, b, |o| o.is_lt())
}

pub fn le(a: Val, b: Val) -> Val {
    compare(a, b, |o| o.is_le())
}

pub fn gt(a: Val, b: Val) -> Val {
    compare(a, b, |o| o.is_gt())
}

pub fn ge(a: Val, b: Val) -> Val {
    compare(a, b, |o| o.is_ge())
}

/// `a in [..]`, as SQL's `IN`: true on a match, null when `a` is null or no
/// item matched but one was null, false otherwise.
pub fn in_list(a: Val, list: Val) -> Val {
    let Val::List(items) = list else { return Val::Null };
    if matches!(a, Val::Null) {
        return Val::Null;
    }
    let mut unknown = false;
    for item in items {
        match eq(a.clone(), item) {
            Val::Bool(true) => return Val::Bool(true),
            Val::Null => unknown = true,
            _ => {}
        }
    }
    if unknown {
        Val::Null
    } else {
        Val::Bool(false)
    }
}

pub fn not_in(a: Val, list: Val) -> Val {
    not(in_list(a, list))
}

/// `a matches 'pattern'`: whether the text contains a match of the pattern.
/// The generator only emits patterns that compile.
pub fn matches(a: Val, pattern: &str) -> Val {
    static CACHE: OnceLock<Mutex<HashMap<String, Option<regex::Regex>>>> = OnceLock::new();
    let Some(s) = a.text() else { return Val::Null };
    let mut cache = CACHE.get_or_init(Default::default).lock().unwrap_or_else(|e| e.into_inner());
    let re = cache.entry(pattern.to_string()).or_insert_with(|| regex::Regex::new(pattern).ok());
    match re {
        Some(re) => Val::Bool(re.is_match(s)),
        None => Val::Null,
    }
}

// ── Arithmetic ───────────────────────────────────────────────────────────

fn arith(a: Val, b: Val, f: fn(f64, f64) -> Option<f64>) -> Val {
    match (a.num(), b.num()) {
        (Some(x), Some(y)) => f(x, y).map(Val::Num).unwrap_or(Val::Null),
        _ => Val::Null,
    }
}

pub fn add(a: Val, b: Val) -> Val {
    arith(a, b, |x, y| Some(x + y))
}

pub fn sub(a: Val, b: Val) -> Val {
    arith(a, b, |x, y| Some(x - y))
}

pub fn mul(a: Val, b: Val) -> Val {
    arith(a, b, |x, y| Some(x * y))
}

/// Division by zero is null rather than an error: a rule never panics a write.
pub fn div(a: Val, b: Val) -> Val {
    arith(a, b, |x, y| (y != 0.0).then(|| x / y))
}

pub fn rem(a: Val, b: Val) -> Val {
    arith(a, b, |x, y| (y != 0.0).then(|| x % y))
}

pub fn neg(a: Val) -> Val {
    a.num().map(|n| Val::Num(-n)).unwrap_or(Val::Null)
}

fn num_fn(a: Val, f: fn(f64) -> f64) -> Val {
    a.num().map(|n| Val::Num(f(n))).unwrap_or(Val::Null)
}

pub fn abs(a: Val) -> Val {
    num_fn(a, f64::abs)
}

pub fn ceil(a: Val) -> Val {
    num_fn(a, f64::ceil)
}

pub fn floor(a: Val) -> Val {
    num_fn(a, f64::floor)
}

pub fn round(a: Val) -> Val {
    num_fn(a, f64::round)
}

/// The first value that is not null.
pub fn coalesce(values: Vec<Val>) -> Val {
    values.into_iter().find(|v| !matches!(v, Val::Null)).unwrap_or(Val::Null)
}

// ── Text and lists ───────────────────────────────────────────────────────

/// A text's length in characters (code points, as on every target), or a list's.
pub fn length(a: Val) -> Val {
    match a {
        Val::Text(s) => Val::Num(s.chars().count() as f64),
        Val::List(items) => Val::Num(items.len() as f64),
        _ => Val::Null,
    }
}

pub fn is_empty(a: Val) -> Val {
    match length(a) {
        Val::Num(n) => Val::Bool(n == 0.0),
        _ => Val::Null,
    }
}

fn text_fn(a: Val, f: fn(&str) -> String) -> Val {
    a.text().map(|s| Val::Text(f(s))).unwrap_or(Val::Null)
}

pub fn trim(a: Val) -> Val {
    text_fn(a, |s| s.trim().to_string())
}

pub fn lower(a: Val) -> Val {
    text_fn(a, str::to_lowercase)
}

pub fn upper(a: Val) -> Val {
    text_fn(a, str::to_uppercase)
}

/// `a.contains(b)`: a text containing a text, or a list containing an item.
pub fn contains(a: Val, b: Val) -> Val {
    match (a, b) {
        (Val::Text(s), Val::Text(t)) => Val::Bool(s.contains(t.as_str())),
        (list @ Val::List(_), item) => in_list(item, list),
        _ => Val::Null,
    }
}

pub fn starts_with(a: Val, b: Val) -> Val {
    match (a.text(), b.text()) {
        (Some(s), Some(t)) => Val::Bool(s.starts_with(t)),
        _ => Val::Null,
    }
}

pub fn ends_with(a: Val, b: Val) -> Val {
    match (a.text(), b.text()) {
        (Some(s), Some(t)) => Val::Bool(s.ends_with(t)),
        _ => Val::Null,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn a_rule_over_a_null_field_holds_as_a_check_constraint_does() {
        let row = json!({ "name": null });
        let rule = ge(length(field(&row, "name")), Val::Num(1.0));
        assert_eq!(rule, Val::Null);
        assert!(holds(&rule));
        // A missing field reads the same as a null one.
        assert!(holds(&ge(length(field(&json!({}), "name")), Val::Num(1.0))));
    }

    #[test]
    fn a_null_test_is_never_null_itself() {
        let row = json!({ "email": null });
        assert_eq!(is_not_null(field(&row, "email")), Val::Bool(false));
        assert!(!holds(&and(is_not_null(field(&row, "email")), matches(field(&row, "email"), "@"))));
    }

    #[test]
    fn logic_is_three_valued() {
        assert_eq!(and(Val::Bool(false), Val::Null), Val::Bool(false));
        assert_eq!(and(Val::Bool(true), Val::Null), Val::Null);
        assert_eq!(or(Val::Bool(true), Val::Null), Val::Bool(true));
        assert_eq!(or(Val::Bool(false), Val::Null), Val::Null);
        assert_eq!(not(Val::Null), Val::Null);
        assert_eq!(cond(Val::Null, Val::Bool(true), Val::Bool(false)), Val::Bool(false));
    }

    #[test]
    fn a_decimal_serialized_as_text_compares_as_a_number() {
        let row = json!({ "risk_score": "0.75", "count": 3 });
        assert_eq!(le(field(&row, "risk_score"), Val::Num(1.0)), Val::Bool(true));
        assert_eq!(gt(field(&row, "risk_score"), Val::Num(1.0)), Val::Bool(false));
        assert_eq!(ge(field(&row, "count"), Val::Num(1.0)), Val::Bool(true));
    }

    #[test]
    fn in_follows_sql_in() {
        let list = Val::List(vec![text("json"), text("csv")]);
        assert_eq!(in_list(text("csv"), list.clone()), Val::Bool(true));
        assert_eq!(in_list(text("pdf"), list.clone()), Val::Bool(false));
        assert_eq!(in_list(Val::Null, list.clone()), Val::Null);
        assert_eq!(not_in(text("pdf"), list), Val::Bool(true));
    }

    #[test]
    fn text_helpers_count_characters_not_bytes() {
        assert_eq!(length(text("héllo")), Val::Num(5.0));
        assert_eq!(length(trim(text("  a "))), Val::Num(1.0));
        assert_eq!(contains(text("a/../b"), text("..")), Val::Bool(true));
        assert_eq!(matches(text("my-bucket_1"), "^[a-zA-Z0-9_-]+$"), Val::Bool(true));
        assert_eq!(matches(text("no spaces"), "^[a-zA-Z0-9_-]+$"), Val::Bool(false));
    }

    #[test]
    fn division_by_zero_is_null_not_a_panic() {
        assert_eq!(div(Val::Num(1.0), Val::Num(0.0)), Val::Null);
        assert_eq!(rem(Val::Num(1.0), Val::Num(0.0)), Val::Null);
    }
}
