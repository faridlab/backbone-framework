//! Request body extractors.
//!
//! [`JsonOrForm`] accepts a request body as EITHER `application/json` OR
//! `application/x-www-form-urlencoded` (defaulting to JSON when the
//! `Content-Type` is absent/unknown), so endpoints built with
//! [`crate::http::BackboneCrudHandler`] — and any handler using it — work with
//! both content types. Drop-in replacement for `Json(x): Json<T>` /
//! `Form(x): Form<T>` in handler signatures and route closures.
//!
//! A body that cannot be read as `T` is refused with the standard JSON error envelope
//! ([`ApiResponse`] with `success: false`), never axum's plain-text rejection: malformed JSON
//! is a 400 with a `malformed_body` violation, and a well-formed body with a wrong or missing
//! value is a 422 whose violation names the field (`lines/3/amount`).
//!
//! [`StrictJsonOrForm`] additionally refuses a JSON body carrying a key `T` does not take
//! (422, code `unknown_field`, the key as path): serde drops such a key silently, and the
//! caller's edit with it.

use axum::{
    async_trait,
    body::Bytes,
    extract::{FromRequest, Request},
    http::{header::CONTENT_TYPE, StatusCode},
    response::{IntoResponse, Response},
    Json,
};
use serde::de::DeserializeOwned;

use crate::http::ApiResponse;
use crate::violation::Violation;

/// Body extractor that accepts JSON or url-encoded form data. Keys the target type does not
/// take are ignored; see [`StrictJsonOrForm`] for the variant that refuses them.
pub struct JsonOrForm<T>(pub T);

/// [`JsonOrForm`] that refuses a JSON body carrying a key the target type does not take.
///
/// The keys a record's own response carries but no write takes — `id`, `metadata` and the
/// `created_at` / `updated_at` / `deleted_at` timestamps, in either casing — are tolerated, so
/// a client that sends back the record it read is not refused for them. Form bodies stay
/// lenient: a browser form posts its buttons and hidden inputs too.
pub struct StrictJsonOrForm<T>(pub T);

#[async_trait]
impl<S, T> FromRequest<S> for JsonOrForm<T>
where
    S: Send + Sync,
    T: DeserializeOwned + 'static,
{
    type Rejection = Response;

    async fn from_request(req: Request, state: &S) -> Result<Self, Self::Rejection> {
        decode_body(req, state, false).await.map(Self)
    }
}

#[async_trait]
impl<S, T> FromRequest<S> for StrictJsonOrForm<T>
where
    S: Send + Sync,
    T: DeserializeOwned + 'static,
{
    type Rejection = Response;

    async fn from_request(req: Request, state: &S) -> Result<Self, Self::Rejection> {
        decode_body(req, state, true).await.map(Self)
    }
}

/// Keys a client may echo back from a record it read although no write takes them.
const ECHOED_KEYS: &[&str] = &[
    "id",
    "metadata",
    "created_at",
    "updated_at",
    "deleted_at",
    "createdAt",
    "updatedAt",
    "deletedAt",
];

async fn decode_body<S, T>(req: Request, state: &S, strict: bool) -> Result<T, Response>
where
    S: Send + Sync,
    T: DeserializeOwned,
{
    let is_form = req
        .headers()
        .get(CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .map(|ct| ct.starts_with("application/x-www-form-urlencoded"))
        .unwrap_or(false);
    let bytes = Bytes::from_request(req, state).await.map_err(|rejection| {
        let status = rejection.status();
        refusal(
            status,
            rejection.body_text(),
            Violation::new("", "unreadable_body", "The request body could not be read"),
        )
    })?;

    if is_form {
        return serde_urlencoded::from_bytes(&bytes).map_err(|e| {
            let message = e.to_string();
            refusal(
                StatusCode::UNPROCESSABLE_ENTITY,
                format!("invalid form body: {message}"),
                Violation::new("", "invalid_value", message),
            )
        });
    }
    decode_json(&bytes, strict)
}

/// A body refused before it reached the service: its status, sentence and violations.
pub(crate) struct Refusal {
    status: StatusCode,
    error: String,
    violations: Vec<Violation>,
}

impl Refusal {
    fn new(status: StatusCode, error: String, violations: Vec<Violation>) -> Self {
        Self {
            status,
            error,
            violations,
        }
    }

    /// Place the refusal on item `index` of a batch body.
    pub(crate) fn at_item(mut self, index: usize) -> Self {
        for v in &mut self.violations {
            v.path = crate::violation::batch_item_path(index, &v.path);
        }
        self.error = format!("item {index}: {}", self.error);
        self
    }

    pub(crate) fn into_response(self) -> Response {
        (
            self.status,
            Json(ApiResponse::<()>::rejected(self.error, self.violations)),
        )
            .into_response()
    }
}

/// Read `bytes` as JSON into `T`, refusing with the field the failure is at; with `strict`,
/// also refusing a key `T` does not take (other than [`ECHOED_KEYS`]).
// A rejection is a `Response`, as every axum extractor's is; it is built once per refused body.
#[allow(clippy::result_large_err)]
pub(crate) fn decode_json<T: DeserializeOwned>(bytes: &[u8], strict: bool) -> Result<T, Response> {
    decode_json_or_refusal(bytes, strict).map_err(Refusal::into_response)
}

/// [`decode_json`], with the refusal still open to be placed on a batch item.
pub(crate) fn decode_json_or_refusal<T: DeserializeOwned>(
    bytes: &[u8],
    strict: bool,
) -> Result<T, Refusal> {
    let mut ignored: Vec<String> = Vec::new();
    let mut de = serde_json::Deserializer::from_slice(bytes);
    let decoded: Result<T, serde_path_to_error::Error<serde_json::Error>> = if strict {
        let mut record = |path: serde_ignored::Path<'_>| {
            if let Some(path) = ignored_key(&path) {
                ignored.push(path);
            }
        };
        let tracked = serde_ignored::Deserializer::new(&mut de, &mut record);
        serde_path_to_error::deserialize(tracked)
    } else {
        serde_path_to_error::deserialize(&mut de)
    };
    let value = match decoded {
        Ok(value) => value,
        Err(e) => {
            let path = path_segments(e.path());
            return Err(json_refusal(path, e.into_inner()));
        }
    };
    // Trailing characters after the value are a malformed body, not a value to keep.
    if let Err(e) = de.end() {
        return Err(json_refusal(Vec::new(), e));
    }
    if !ignored.is_empty() {
        ignored.sort();
        ignored.dedup();
        let violations = ignored
            .iter()
            .map(|path| {
                let key = path.rsplit('/').next().unwrap_or(path);
                Violation::new(
                    path.clone(),
                    "unknown_field",
                    format!("unknown_field: `{key}` is not a field this write accepts"),
                )
            })
            .collect::<Vec<_>>();
        let sentence = violations
            .iter()
            .map(|v| v.message.as_str())
            .collect::<Vec<_>>()
            .join("; ");
        return Err(Refusal::new(
            StatusCode::UNPROCESSABLE_ENTITY,
            format!("validation failed: {sentence}"),
            violations,
        ));
    }
    Ok(value)
}

/// The slash path of a key serde ignored, unless it is one a client may echo back.
fn ignored_key(path: &serde_ignored::Path<'_>) -> Option<String> {
    let mut segments = Vec::new();
    collect_segments(path, &mut segments);
    let echoed = segments.len() <= 2
        && segments
            .last()
            .is_some_and(|(key, is_key)| *is_key && ECHOED_KEYS.contains(&key.as_str()))
        && (segments.len() == 1 || !segments[0].1);
    if echoed || segments.is_empty() {
        return None;
    }
    Some(
        segments
            .into_iter()
            .map(|(s, _)| s)
            .collect::<Vec<_>>()
            .join("/"),
    )
}

/// Flatten an ignored-key path into `(segment, is_map_key)` from the root down.
fn collect_segments(path: &serde_ignored::Path<'_>, out: &mut Vec<(String, bool)>) {
    use serde_ignored::Path;
    match path {
        Path::Root => {}
        Path::Seq { parent, index } => {
            collect_segments(parent, out);
            out.push((index.to_string(), false));
        }
        Path::Map { parent, key } => {
            collect_segments(parent, out);
            out.push((key.clone(), true));
        }
        Path::Some { parent }
        | Path::NewtypeStruct { parent }
        | Path::NewtypeVariant { parent } => collect_segments(parent, out),
    }
}

/// The segments of the path serde failed at, root first.
fn path_segments(path: &serde_path_to_error::Path) -> Vec<String> {
    path.iter()
        .filter_map(|segment| match segment {
            serde_path_to_error::Segment::Seq { index } => Some(index.to_string()),
            serde_path_to_error::Segment::Map { key } => Some(key.clone()),
            serde_path_to_error::Segment::Enum { variant } => Some(variant.clone()),
            serde_path_to_error::Segment::Unknown => None,
        })
        .collect()
}

/// The refusal for a JSON body serde could not read at `path`.
fn json_refusal(mut path: Vec<String>, inner: serde_json::Error) -> Refusal {
    use serde_json::error::Category;
    let message = without_position(&inner.to_string());
    match inner.classify() {
        Category::Syntax | Category::Eof | Category::Io => Refusal::new(
            StatusCode::BAD_REQUEST,
            format!("malformed JSON body: {message}"),
            vec![Violation::new(
                "",
                "malformed_body",
                format!("The request body is not valid JSON: {message}"),
            )],
        ),
        Category::Data => {
            let (code, text) = match missing_field(&message) {
                Some(field) => {
                    path.push(field.to_string());
                    ("required", format!("{field} is required"))
                }
                None => ("invalid_value", message.clone()),
            };
            let path = path.join("/");
            let sentence = if path.is_empty() {
                text.clone()
            } else {
                format!("{path}: {text}")
            };
            Refusal::new(
                StatusCode::UNPROCESSABLE_ENTITY,
                format!("validation failed: {sentence}"),
                vec![Violation::new(path, code, text)],
            )
        }
    }
}

/// The field a serde "missing field `x`" message names.
fn missing_field(message: &str) -> Option<&str> {
    message.strip_prefix("missing field `")?.split('`').next()
}

/// Drop serde_json's " at line N column M" suffix: the client has no use for byte positions
/// in a body it built as an object.
fn without_position(message: &str) -> String {
    match message.rfind(" at line ") {
        Some(at) => message[..at].to_string(),
        None => message.to_string(),
    }
}

fn refusal(status: StatusCode, error: String, violation: Violation) -> Response {
    (
        status,
        Json(ApiResponse::<()>::rejected(error, vec![violation])),
    )
        .into_response()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde::Deserialize;

    #[derive(Debug, Deserialize)]
    #[serde(rename_all = "camelCase")]
    #[allow(dead_code)]
    struct Line {
        amount: i64,
        #[serde(alias = "unit_price", default)]
        unit_price: Option<i64>,
    }

    #[derive(Debug, Deserialize)]
    #[allow(dead_code)]
    struct Doc {
        name: String,
        #[serde(default)]
        lines: Vec<Line>,
    }

    async fn body_of(r: Response) -> (StatusCode, serde_json::Value) {
        let status = r.status();
        let bytes = axum::body::to_bytes(r.into_body(), usize::MAX)
            .await
            .unwrap();
        (status, serde_json::from_slice(&bytes).unwrap())
    }

    #[tokio::test]
    async fn malformed_json_is_a_400_envelope() {
        let (status, body) = body_of(decode_json::<Doc>(b"{\"name\": ", false).unwrap_err()).await;
        assert_eq!(status, StatusCode::BAD_REQUEST);
        assert_eq!(body["success"], false);
        assert_eq!(body["violations"][0]["code"], "malformed_body");
    }

    #[tokio::test]
    async fn a_wrong_value_names_its_field() {
        let raw = br#"{"name":"a","lines":[{"amount":1},{"amount":"x"}]}"#;
        let (status, body) = body_of(decode_json::<Doc>(raw, false).unwrap_err()).await;
        assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
        assert_eq!(body["violations"][0]["path"], "lines/1/amount");
        assert_eq!(body["violations"][0]["code"], "invalid_value");
    }

    #[tokio::test]
    async fn a_missing_field_is_required_on_that_field() {
        let (status, body) =
            body_of(decode_json::<Doc>(br#"{"lines":[]}"#, false).unwrap_err()).await;
        assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
        assert_eq!(body["violations"][0]["path"], "name");
        assert_eq!(body["violations"][0]["code"], "required");
    }

    #[tokio::test]
    async fn strict_refuses_a_key_the_type_does_not_take() {
        let raw = br#"{"name":"a","colour":"red","lines":[{"amount":1,"discount":2}]}"#;
        assert!(
            decode_json::<Doc>(raw, false).is_ok(),
            "lenient decoding ignores the key"
        );
        let (status, body) = body_of(decode_json::<Doc>(raw, true).unwrap_err()).await;
        assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
        let paths: Vec<&str> = body["violations"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v["path"].as_str().unwrap())
            .collect();
        assert_eq!(paths, vec!["colour", "lines/0/discount"]);
        assert_eq!(body["violations"][0]["code"], "unknown_field");
    }

    #[test]
    fn strict_tolerates_what_a_response_echoes_and_both_casings() {
        let raw = br#"{"id":"x","metadata":{},"createdAt":"t","name":"a","lines":[{"amount":1,"unitPrice":2},{"amount":1,"unit_price":3}]}"#;
        assert!(decode_json::<Doc>(raw, true).is_ok());
        // Echoed keys are tolerated on each item of a batch too.
        assert!(decode_json::<Vec<Doc>>(br#"[{"id":"x","name":"a"}]"#, true).is_ok());
        let err = decode_json::<Vec<Doc>>(br#"[{"name":"a"},{"name":"b","bogus":1}]"#, true);
        assert!(err.is_err());
    }
}
