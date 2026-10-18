//! The closed expression set a workflow runs.
//!
//! The schema generator parses every condition, `set:` value and `{{ }}` template of a workflow
//! when it generates, refuses what is not in this set, and writes the result as the data types of
//! this module (an [`Expr`] tree, a [`Template`], a [`Tpl`] value). The engine evaluates them over
//! the instance's JSON context; nothing is parsed at run time.
//!
//! What an expression can say:
//!
//! - paths into `context`, `event`, `result`, `error` and `config`, with `.key` and `[n]`, and
//!   `steps.<name>.completed`; a missing key reads as `null`;
//! - literals (`null`, booleans, numbers, strings, arrays) and duration literals (`15m`, `30d`);
//! - `== != < <= > >=`, `&& || !`, `+ - * / %`, `a ?? b`, `a ? b : c`, `a in [..]`;
//! - `.length`, `.contains(x)`, `.includes(x)`, `.starts_with(x)`, `.ends_with(x)`;
//! - `now()`, `new_uuid()`, `coalesce(a, b, ..)`, `len(x)`;
//! - a function the workflow declares under `functions:`, called through [`Funcs`].
//!
//! `time + duration` and `time - duration` add to an RFC 3339 text and answer one; `<` and friends
//! compare two RFC 3339 texts as instants.

use std::collections::HashSet;
use std::future::Future;
use std::pin::Pin;

use async_trait::async_trait;
use chrono::{DateTime, Duration, SecondsFormat, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Where a path starts.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Root {
    /// The instance's context.
    Context,
    /// The payload of the event being handled (the start event, or the one a wait received).
    Event,
    /// The result of the step that just ran (else the last stored one, `context.result`).
    Result,
    /// The message of the failure a step was routed on.
    Error,
    /// The workflow's configuration the generator exposes.
    Config,
    /// The completed steps: `steps.<name>.completed`.
    Steps,
}

/// One step of a path.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Seg {
    /// `.name`
    Key(String),
    /// `[n]`
    Index(usize),
}

/// A binary operator.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BinOp {
    /// `==`
    Eq,
    /// `!=`
    Ne,
    /// `<`
    Lt,
    /// `<=`
    Le,
    /// `>`
    Gt,
    /// `>=`
    Ge,
    /// `&&`, short-circuiting.
    And,
    /// `||`, short-circuiting.
    Or,
    /// `+`
    Add,
    /// `-`
    Sub,
    /// `*`
    Mul,
    /// `/`
    Div,
    /// `%`
    Mod,
}

/// A compiled expression.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Expr {
    /// A literal JSON value.
    Lit(Value),
    /// A duration literal, in seconds. It adds to or subtracts from a time.
    Dur(i64),
    /// `now()`: the time the step runs, as RFC 3339 text.
    Now,
    /// A path.
    Path {
        /// Where it starts.
        root: Root,
        /// What it walks.
        path: Vec<Seg>,
    },
    /// `!x`
    Not(Box<Expr>),
    /// `-x`
    Neg(Box<Expr>),
    /// `l op r`
    Bin {
        /// The operator.
        op: BinOp,
        /// Left side.
        l: Box<Expr>,
        /// Right side.
        r: Box<Expr>,
    },
    /// `l ?? r`: `l`, unless it is `null`.
    Coalesce(Box<Expr>, Box<Expr>),
    /// `c ? t : e`
    If {
        /// The condition.
        c: Box<Expr>,
        /// When it holds.
        t: Box<Expr>,
        /// When it does not.
        e: Box<Expr>,
    },
    /// `[a, b]`
    Array(Vec<Expr>),
    /// `l in r` (or `not in` when `negate`): membership of an array, a substring of a string.
    In {
        /// What is looked for.
        l: Box<Expr>,
        /// What is searched.
        r: Box<Expr>,
        /// `not in`.
        negate: bool,
    },
    /// `obj.name(args)`: `length`, `contains`, `includes`, `starts_with`, `ends_with`.
    Method {
        /// The receiver.
        obj: Box<Expr>,
        /// The method.
        name: String,
        /// Its arguments.
        args: Vec<Expr>,
    },
    /// `name(args)`: `new_uuid`, `coalesce`, `len`, or a function the workflow declares.
    Call {
        /// The function.
        name: String,
        /// Its arguments.
        args: Vec<Expr>,
    },
}

/// A piece of a template.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Part {
    /// Text as written.
    Text(String),
    /// A `{{ }}` expression.
    Expr(Expr),
}

/// A string with `{{ }}` holes. A template that is one hole renders as that expression's own value
/// (a number stays a number, an object stays an object); anything else renders as text.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Template(pub Vec<Part>);

/// A value of a step's parameters: a literal, a template, or a map or list of them.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Tpl {
    /// A literal.
    Lit(Value),
    /// A template.
    Template(Template),
    /// A map, in the order written.
    Map(Vec<(String, Tpl)>),
    /// A list.
    List(Vec<Tpl>),
}

/// A duration: fixed, or a template evaluated when it is needed (`{{ context.minutes }}m`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Dur {
    /// Seconds.
    Secs(i64),
    /// Rendered, then read with [`parse_duration`].
    Tpl(Template),
}

/// What an expression reads.
pub struct Scope<'a> {
    /// The instance's context.
    pub context: &'a Value,
    /// The event being handled.
    pub event: &'a Value,
    /// The result of the step that just ran.
    pub result: &'a Value,
    /// The failure message a step was routed on.
    pub error: &'a Value,
    /// The workflow's configuration.
    pub config: &'a Value,
    /// The names of the steps that completed.
    pub completed: &'a HashSet<String>,
    /// When the step runs.
    pub now: DateTime<Utc>,
}

/// The functions a workflow declares under `functions:`, implemented by hand.
#[async_trait]
pub trait Funcs: Send + Sync {
    /// Call the declared function `name`. An error fails the step that evaluated it.
    async fn call(&self, name: &str, args: &[Value]) -> Result<Value, String>;
}

/// A [`Funcs`] that declares nothing: every call is an error.
pub struct NoFuncs;

#[async_trait]
impl Funcs for NoFuncs {
    async fn call(&self, name: &str, _args: &[Value]) -> Result<Value, String> {
        Err(format!("function `{name}` is not declared"))
    }
}

type Eval<'a> = Pin<Box<dyn Future<Output = Result<Value, String>> + Send + 'a>>;

/// Whether `v` counts as true: not `null`, `false`, `0` or `""`.
pub fn truthy(v: &Value) -> bool {
    match v {
        Value::Null => false,
        Value::Bool(b) => *b,
        Value::Number(n) => n.as_f64().map(|f| f != 0.0).unwrap_or(true),
        Value::String(s) => !s.is_empty(),
        Value::Array(_) | Value::Object(_) => true,
    }
}

/// Evaluate `expr` over `scope`.
pub fn eval<'a>(expr: &'a Expr, scope: &'a Scope<'a>, funcs: &'a dyn Funcs) -> Eval<'a> {
    Box::pin(async move {
        match expr {
            Expr::Lit(v) => Ok(v.clone()),
            Expr::Dur(s) => Ok(Value::from(*s)),
            Expr::Now => Ok(Value::String(time_text(scope.now))),
            Expr::Path { root, path } => Ok(read_path(*root, path, scope)),
            Expr::Not(e) => Ok(Value::Bool(!truthy(&eval(e, scope, funcs).await?))),
            Expr::Neg(e) => {
                let v = eval(e, scope, funcs).await?;
                number(&v)
                    .map(|n| num_value(-n))
                    .ok_or_else(|| "`-` needs a number".to_string())
            }
            Expr::Bin {
                op: BinOp::And,
                l,
                r,
            } => {
                let left = eval(l, scope, funcs).await?;
                if !truthy(&left) {
                    return Ok(Value::Bool(false));
                }
                Ok(Value::Bool(truthy(&eval(r, scope, funcs).await?)))
            }
            Expr::Bin {
                op: BinOp::Or,
                l,
                r,
            } => {
                let left = eval(l, scope, funcs).await?;
                if truthy(&left) {
                    return Ok(Value::Bool(true));
                }
                Ok(Value::Bool(truthy(&eval(r, scope, funcs).await?)))
            }
            Expr::Bin { op, l, r } => {
                let left = eval(l, scope, funcs).await?;
                let right = eval(r, scope, funcs).await?;
                binary(*op, &left, &right, r)
            }
            Expr::Coalesce(l, r) => {
                let left = eval(l, scope, funcs).await?;
                if left.is_null() {
                    eval(r, scope, funcs).await
                } else {
                    Ok(left)
                }
            }
            Expr::If { c, t, e } => {
                if truthy(&eval(c, scope, funcs).await?) {
                    eval(t, scope, funcs).await
                } else {
                    eval(e, scope, funcs).await
                }
            }
            Expr::Array(items) => {
                let mut out = Vec::with_capacity(items.len());
                for item in items {
                    out.push(eval(item, scope, funcs).await?);
                }
                Ok(Value::Array(out))
            }
            Expr::In { l, r, negate } => {
                let needle = eval(l, scope, funcs).await?;
                let hay = eval(r, scope, funcs).await?;
                Ok(Value::Bool(contains(&hay, &needle) != *negate))
            }
            Expr::Method { obj, name, args } => {
                let recv = eval(obj, scope, funcs).await?;
                let mut vals = Vec::with_capacity(args.len());
                for a in args {
                    vals.push(eval(a, scope, funcs).await?);
                }
                method(&recv, name, &vals)
            }
            Expr::Call { name, args } => {
                let mut vals = Vec::with_capacity(args.len());
                for a in args {
                    vals.push(eval(a, scope, funcs).await?);
                }
                match name.as_str() {
                    "new_uuid" => Ok(Value::String(uuid::Uuid::new_v4().to_string())),
                    "coalesce" => Ok(vals
                        .into_iter()
                        .find(|v| !v.is_null())
                        .unwrap_or(Value::Null)),
                    "len" => method(vals.first().unwrap_or(&Value::Null), "length", &[]),
                    other => funcs.call(other, &vals).await,
                }
            }
        }
    })
}

/// Evaluate `expr` and read the answer as a condition.
pub async fn holds(expr: &Expr, scope: &Scope<'_>, funcs: &dyn Funcs) -> Result<bool, String> {
    Ok(truthy(&eval(expr, scope, funcs).await?))
}

/// Render a template: one hole is that expression's value, anything else is text.
pub async fn render_template(
    t: &Template,
    scope: &Scope<'_>,
    funcs: &dyn Funcs,
) -> Result<Value, String> {
    if let [Part::Expr(e)] = t.0.as_slice() {
        return eval(e, scope, funcs).await;
    }
    let mut out = String::new();
    for part in &t.0 {
        match part {
            Part::Text(s) => out.push_str(s),
            Part::Expr(e) => out.push_str(&text_of(&eval(e, scope, funcs).await?)),
        }
    }
    Ok(Value::String(out))
}

/// Render a parameter value.
pub fn render<'a>(tpl: &'a Tpl, scope: &'a Scope<'a>, funcs: &'a dyn Funcs) -> Eval<'a> {
    Box::pin(async move {
        match tpl {
            Tpl::Lit(v) => Ok(v.clone()),
            Tpl::Template(t) => render_template(t, scope, funcs).await,
            Tpl::Map(entries) => {
                let mut map = serde_json::Map::new();
                for (k, v) in entries {
                    map.insert(k.clone(), render(v, scope, funcs).await?);
                }
                Ok(Value::Object(map))
            }
            Tpl::List(items) => {
                let mut out = Vec::with_capacity(items.len());
                for item in items {
                    out.push(render(item, scope, funcs).await?);
                }
                Ok(Value::Array(out))
            }
        }
    })
}

/// Render a duration: seconds, or a template read with [`parse_duration`].
pub async fn render_duration(d: &Dur, scope: &Scope<'_>, funcs: &dyn Funcs) -> Result<i64, String> {
    match d {
        Dur::Secs(s) => Ok(*s),
        Dur::Tpl(t) => {
            let v = render_template(t, scope, funcs).await?;
            match &v {
                Value::Number(n) => n
                    .as_i64()
                    .ok_or_else(|| format!("`{n}` is not a whole number of seconds")),
                Value::String(s) => parse_duration(s)
                    .ok_or_else(|| format!("`{s}` is not a duration (use 30s, 15m, 24h, 7d)")),
                other => Err(format!("`{other}` is not a duration")),
            }
        }
    }
}

/// Read `30s`, `15m`, `24h`, `7d`, `2w`, or a bare number of seconds, as seconds.
pub fn parse_duration(text: &str) -> Option<i64> {
    let t = text.trim();
    if !t.is_empty() && t.chars().all(|c| c.is_ascii_digit()) {
        return t.parse().ok();
    }
    let split = t.find(|c: char| !c.is_ascii_digit())?;
    let (digits, unit) = t.split_at(split);
    let n: i64 = digits.parse().ok()?;
    let secs = match unit {
        "s" => 1,
        "m" => 60,
        "h" => 3600,
        "d" => 86_400,
        "w" => 604_800,
        _ => return None,
    };
    n.checked_mul(secs)
}

/// The key a wait is matched on: the canonical text of each value, in the order given, as a JSON
/// array. Both the instance (from its context) and the event (from its payload) build it the same
/// way, so equal values make equal keys.
pub fn wait_key(values: &[Value]) -> String {
    let parts: Vec<String> = values.iter().map(canonical).collect();
    serde_json::to_string(&parts).unwrap_or_default()
}

/// The canonical text of a value as a wait key: strings as they are, `null` as `null`, a whole
/// number without a fraction.
pub fn canonical(v: &Value) -> String {
    match v {
        Value::String(s) => s.clone(),
        Value::Number(n) => match n.as_f64() {
            Some(f) if f.fract() == 0.0 && f.abs() < 9e15 => format!("{}", f as i64),
            _ => n.to_string(),
        },
        other => other.to_string(),
    }
}

fn read_path(root: Root, path: &[Seg], scope: &Scope<'_>) -> Value {
    if root == Root::Steps {
        return match path {
            [Seg::Key(name), Seg::Key(field)] if field == "completed" => {
                Value::Bool(scope.completed.contains(name))
            }
            _ => Value::Null,
        };
    }
    let mut cur = match root {
        Root::Context => scope.context,
        Root::Event => scope.event,
        Root::Result => scope.result,
        Root::Error => scope.error,
        Root::Config => scope.config,
        Root::Steps => unreachable!(),
    };
    for seg in path {
        cur = match (seg, cur) {
            (Seg::Key(k), Value::Object(m)) => match m.get(k) {
                Some(v) => v,
                None => return Value::Null,
            },
            (Seg::Index(i), Value::Array(a)) => match a.get(*i) {
                Some(v) => v,
                None => return Value::Null,
            },
            _ => return Value::Null,
        };
    }
    cur.clone()
}

fn time_text(t: DateTime<Utc>) -> String {
    t.to_rfc3339_opts(SecondsFormat::Micros, true)
}

fn parse_time(v: &Value) -> Option<DateTime<Utc>> {
    v.as_str()
        .and_then(|s| DateTime::parse_from_rfc3339(s).ok())
        .map(|d| d.with_timezone(&Utc))
}

fn number(v: &Value) -> Option<f64> {
    match v {
        Value::Number(n) => n.as_f64(),
        _ => None,
    }
}

fn num_value(f: f64) -> Value {
    if f.fract() == 0.0 && f.abs() < 9e15 {
        Value::from(f as i64)
    } else {
        serde_json::Number::from_f64(f)
            .map(Value::Number)
            .unwrap_or(Value::Null)
    }
}

fn json_eq(a: &Value, b: &Value) -> bool {
    match (a, b) {
        (Value::Number(x), Value::Number(y)) => x.as_f64() == y.as_f64(),
        _ => a == b,
    }
}

fn contains(hay: &Value, needle: &Value) -> bool {
    match hay {
        Value::Array(items) => items.iter().any(|i| json_eq(i, needle)),
        Value::String(s) => needle.as_str().is_some_and(|n| s.contains(n)),
        _ => false,
    }
}

fn method(recv: &Value, name: &str, args: &[Value]) -> Result<Value, String> {
    match name {
        "length" | "len" => Ok(match recv {
            Value::String(s) => Value::from(s.chars().count() as i64),
            Value::Array(a) => Value::from(a.len() as i64),
            Value::Object(m) => Value::from(m.len() as i64),
            _ => Value::from(0),
        }),
        "contains" | "includes" => Ok(Value::Bool(contains(
            recv,
            args.first().unwrap_or(&Value::Null),
        ))),
        "starts_with" => Ok(Value::Bool(
            match (recv.as_str(), args.first().and_then(Value::as_str)) {
                (Some(s), Some(p)) => s.starts_with(p),
                _ => false,
            },
        )),
        "ends_with" => Ok(Value::Bool(
            match (recv.as_str(), args.first().and_then(Value::as_str)) {
                (Some(s), Some(p)) => s.ends_with(p),
                _ => false,
            },
        )),
        other => Err(format!(
            "method `.{other}` is not part of the workflow expression set"
        )),
    }
}

fn binary(op: BinOp, l: &Value, r: &Value, right_expr: &Expr) -> Result<Value, String> {
    match op {
        BinOp::Eq => Ok(Value::Bool(json_eq(l, r))),
        BinOp::Ne => Ok(Value::Bool(!json_eq(l, r))),
        BinOp::Lt | BinOp::Le | BinOp::Gt | BinOp::Ge => {
            let ord = if let (Some(x), Some(y)) = (number(l), number(r)) {
                x.partial_cmp(&y)
            } else if let (Some(x), Some(y)) = (parse_time(l), parse_time(r)) {
                Some(x.cmp(&y))
            } else if let (Some(x), Some(y)) = (l.as_str(), r.as_str()) {
                Some(x.cmp(y))
            } else {
                None
            };
            Ok(Value::Bool(match (ord, op) {
                (None, _) => false,
                (Some(o), BinOp::Lt) => o.is_lt(),
                (Some(o), BinOp::Le) => o.is_le(),
                (Some(o), BinOp::Gt) => o.is_gt(),
                (Some(o), _) => o.is_ge(),
            }))
        }
        BinOp::Add | BinOp::Sub => {
            // A time moved by a duration literal.
            if let (Some(t), Expr::Dur(secs)) = (parse_time(l), right_expr) {
                let delta = Duration::seconds(if op == BinOp::Add { *secs } else { -*secs });
                return Ok(Value::String(time_text(t + delta)));
            }
            if op == BinOp::Add {
                if let (Value::String(a), Value::String(b)) = (l, r) {
                    return Ok(Value::String(format!("{a}{b}")));
                }
            }
            match (number(l), number(r)) {
                (Some(x), Some(y)) => Ok(num_value(if op == BinOp::Add { x + y } else { x - y })),
                _ => Err(format!(
                    "`{}` needs two numbers",
                    if op == BinOp::Add { "+" } else { "-" }
                )),
            }
        }
        BinOp::Mul | BinOp::Div | BinOp::Mod => match (number(l), number(r)) {
            (Some(x), Some(y)) => {
                if op != BinOp::Mul && y == 0.0 {
                    return Err("division by zero".into());
                }
                Ok(num_value(match op {
                    BinOp::Mul => x * y,
                    BinOp::Div => x / y,
                    _ => x % y,
                }))
            }
            _ => Err("arithmetic needs two numbers".into()),
        },
        BinOp::And | BinOp::Or => unreachable!("short-circuited by eval"),
    }
}

/// The text of a value inside a larger template: strings as they are, `null` as nothing.
pub fn text_of(v: &Value) -> String {
    match v {
        Value::Null => String::new(),
        Value::String(s) => s.clone(),
        other => other.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn path(root: Root, keys: &[&str]) -> Expr {
        Expr::Path {
            root,
            path: keys.iter().map(|k| Seg::Key(k.to_string())).collect(),
        }
    }

    fn lit(v: Value) -> Box<Expr> {
        Box::new(Expr::Lit(v))
    }

    async fn run(e: &Expr, context: Value) -> Value {
        let done: HashSet<String> = ["a".to_string()].into();
        let scope = Scope {
            context: &context,
            event: &json!({ "user_id": "u1" }),
            result: &json!({ "id": 7 }),
            error: &Value::Null,
            config: &json!({}),
            completed: &done,
            now: DateTime::parse_from_rfc3339("2026-01-01T00:00:00Z")
                .unwrap()
                .with_timezone(&Utc),
        };
        eval(e, &scope, &NoFuncs).await.unwrap()
    }

    #[tokio::test]
    async fn paths_read_the_scope_and_a_missing_key_is_null() {
        let ctx = json!({ "user": { "id": "u1", "roles": ["a", "b"] } });
        assert_eq!(
            run(&path(Root::Context, &["user", "id"]), ctx.clone()).await,
            json!("u1")
        );
        assert_eq!(
            run(
                &path(Root::Context, &["user", "nope", "deeper"]),
                ctx.clone()
            )
            .await,
            Value::Null
        );
        assert_eq!(
            run(&path(Root::Event, &["user_id"]), ctx.clone()).await,
            json!("u1")
        );
        assert_eq!(
            run(&path(Root::Result, &["id"]), ctx.clone()).await,
            json!(7)
        );
        let idx = Expr::Path {
            root: Root::Context,
            path: vec![
                Seg::Key("user".into()),
                Seg::Key("roles".into()),
                Seg::Index(1),
            ],
        };
        assert_eq!(run(&idx, ctx.clone()).await, json!("b"));
        assert_eq!(
            run(&path(Root::Steps, &["a", "completed"]), ctx.clone()).await,
            json!(true)
        );
        assert_eq!(
            run(&path(Root::Steps, &["b", "completed"]), ctx).await,
            json!(false)
        );
    }

    #[tokio::test]
    async fn comparison_logic_and_coalesce_follow_json_truthiness() {
        let eq = Expr::Bin {
            op: BinOp::Eq,
            l: lit(json!(1)),
            r: lit(json!(1.0)),
        };
        assert_eq!(run(&eq, json!({})).await, json!(true), "1 equals 1.0");
        let and = Expr::Bin {
            op: BinOp::And,
            l: Box::new(path(Root::Context, &["missing"])),
            r: Box::new(Expr::Call {
                name: "boom".into(),
                args: vec![],
            }),
        };
        assert_eq!(
            run(&and, json!({})).await,
            json!(false),
            "`&&` short-circuits past a function that would fail"
        );
        let co = Expr::Coalesce(Box::new(path(Root::Context, &["x"])), lit(json!("d")));
        assert_eq!(run(&co, json!({})).await, json!("d"));
        assert_eq!(run(&co, json!({ "x": "set" })).await, json!("set"));
        let inn = Expr::In {
            l: lit(json!("low")),
            r: lit(json!(["low", "medium"])),
            negate: false,
        };
        assert_eq!(run(&inn, json!({})).await, json!(true));
    }

    #[tokio::test]
    async fn time_arithmetic_and_comparison_use_instants() {
        let plus = Expr::Bin {
            op: BinOp::Add,
            l: Box::new(Expr::Now),
            r: Box::new(Expr::Dur(86_400)),
        };
        assert_eq!(
            run(&plus, json!({})).await,
            json!("2026-01-02T00:00:00.000000Z")
        );
        let later = Expr::Bin {
            op: BinOp::Ge,
            l: Box::new(Expr::Now),
            r: lit(json!("2025-12-31T23:00:00+00:00")),
        };
        assert_eq!(run(&later, json!({})).await, json!(true));
    }

    #[tokio::test]
    async fn methods_and_builtin_functions() {
        let ctx = json!({ "mime": "image/png", "list": [1, 2, 3] });
        let starts = Expr::Method {
            obj: Box::new(path(Root::Context, &["mime"])),
            name: "starts_with".into(),
            args: vec![Expr::Lit(json!("image/"))],
        };
        assert_eq!(run(&starts, ctx.clone()).await, json!(true));
        let len = Expr::Method {
            obj: Box::new(path(Root::Context, &["list"])),
            name: "length".into(),
            args: vec![],
        };
        assert_eq!(run(&len, ctx.clone()).await, json!(3));
        let id = run(
            &Expr::Call {
                name: "new_uuid".into(),
                args: vec![],
            },
            ctx.clone(),
        )
        .await;
        assert!(uuid::Uuid::parse_str(id.as_str().unwrap()).is_ok());
        let co = Expr::Call {
            name: "coalesce".into(),
            args: vec![Expr::Lit(Value::Null), Expr::Lit(json!(5))],
        };
        assert_eq!(run(&co, ctx).await, json!(5));
    }

    #[tokio::test]
    async fn a_template_of_one_hole_keeps_the_type_and_mixed_text_is_text() {
        let context = json!({ "n": 5, "who": "ann" });
        let scope = Scope {
            context: &context,
            event: &Value::Null,
            result: &Value::Null,
            error: &Value::Null,
            config: &Value::Null,
            completed: &HashSet::new(),
            now: Utc::now(),
        };
        let hole = Template(vec![Part::Expr(path(Root::Context, &["n"]))]);
        assert_eq!(
            render_template(&hole, &scope, &NoFuncs).await.unwrap(),
            json!(5)
        );
        let mixed = Template(vec![
            Part::Text("hi ".into()),
            Part::Expr(path(Root::Context, &["who"])),
            Part::Text(" #".into()),
            Part::Expr(path(Root::Context, &["n"])),
        ]);
        assert_eq!(
            render_template(&mixed, &scope, &NoFuncs).await.unwrap(),
            json!("hi ann #5")
        );
        let tpl = Tpl::Map(vec![
            ("a".into(), Tpl::Template(hole)),
            ("b".into(), Tpl::List(vec![Tpl::Lit(json!(1))])),
        ]);
        assert_eq!(
            render(&tpl, &scope, &NoFuncs).await.unwrap(),
            json!({ "a": 5, "b": [1] })
        );
    }

    #[test]
    fn durations_read_with_a_unit_and_wait_keys_are_canonical() {
        assert_eq!(parse_duration("15m"), Some(900));
        assert_eq!(parse_duration("30d"), Some(2_592_000));
        assert_eq!(parse_duration("120"), Some(120));
        assert_eq!(parse_duration("soon"), None);
        assert_eq!(parse_duration("5x"), None);
        assert_eq!(
            wait_key(&[json!("u1"), json!(3)]),
            wait_key(&[json!("u1"), json!(3.0)])
        );
        assert_ne!(wait_key(&[json!("u1")]), wait_key(&[json!("u2")]));
    }

    #[test]
    fn compiled_expressions_survive_their_json_form() {
        let e = Expr::If {
            c: Box::new(path(Root::Context, &["a"])),
            t: lit(json!("x")),
            e: Box::new(Expr::Bin {
                op: BinOp::Add,
                l: Box::new(Expr::Now),
                r: Box::new(Expr::Dur(60)),
            }),
        };
        let text = serde_json::to_string(&e).unwrap();
        assert_eq!(serde_json::from_str::<Expr>(&text).unwrap(), e);
    }
}
