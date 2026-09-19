//! Keyset (cursor) paging for the generic list endpoint.
//!
//! Offset paging makes the database walk and discard every row before the
//! window, so the cost of a page grows with depth: on a table of a million
//! rows, page 25,000 pays for a scan of the 1.25 million rows before it just
//! to hand back fifty. Keyset paging instead walks an index from where the
//! reader stopped — the last row's sort values — so the deep page costs what
//! the first one costs.
//!
//! The cursor is an OPAQUE string the client passes back verbatim:
//!
//! * `after=<cursor>` — the next page, strictly after the cursor's position
//!   in the requested sort order;
//! * `before=<cursor>` — the previous page: the query walks backwards (the
//!   keyset comparison and the ORDER BY are inverted) and the rows are
//!   reversed before returning, so the page always reads forward.
//!
//! Encoding: base64url(JSON) of the sort field names, their directions, the
//! boundary row's values (as text) and its tiebreaker id. The names and
//! directions ride along so a cursor replayed against a DIFFERENT sort is
//! refused rather than silently filtering on the wrong comparison — a cursor
//! is only meaningful for the order it was cut in.
//!
//! The tiebreaker: the boundary row's `id`. Every sort the generic surface
//! offers is made deterministic by appending `id` as the final key, so the
//! same row set always pages the same way and no row is skipped or repeated
//! across a page boundary when sort values tie.
//!
//! The comparison: the textbook expanded predicate, correct under mixed
//! directions where a single row-value comparison is not expressible:
//!
//! ```sql
//! (f1 > $1) OR (f1 = $1 AND f2 < $2) OR (f1 = $1 AND f2 = $2 AND id > $3)
//! ```
//!
//! with `>` / `<` per column chosen by that column's sort direction, and each
//! placeholder cast to the column's real type so the bind (a text string)
//! compares against the column without coercing the COLUMN — casting the
//! column would defeat the index this whole scheme exists to ride.

use serde::{Deserialize, Serialize};

use super::types::SortDirection;
    use serde_json::json;

/// Cursor payload version. A decoded cursor carrying any other version is
/// refused: an old cursor after a format change is a stale client, not a
/// query to guess at.
const CURSOR_VERSION: u8 = 1;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CursorPayload {
    pub v: u8,
    /// The sort field names the cursor was cut in, in order.
    pub f: Vec<String>,
    /// The direction of each sort field: 0 ascending, 1 descending.
    pub d: Vec<u8>,
    /// The boundary row's value for each sort field, as text.
    pub k: Vec<String>,
    /// The boundary row's tiebreaker id.
    pub id: String,
}

/// Why a cursor could not be used. Every arm is a client error: the caller
/// handed back something this endpoint never issued.
#[derive(Debug, thiserror::Error)]
pub enum CursorError {
    #[error("the cursor is not valid base64url JSON: {0}")]
    Malformed(String),
    #[error("the cursor was cut for version {0}; this endpoint issues version {CURSOR_VERSION}")]
    Version(u8),
    #[error(
        "the cursor was cut for a different sort ({expected}); a cursor only pages the order \
         it was issued in"
    )]
    SortMismatch { expected: String },
    #[error("the cursor payload is inconsistent: {0}")]
    Inconsistent(String),
}

/// Encode a cursor for one boundary row.
///
/// `sorts` and `values` must be parallel; the tiebreaker `id` is appended by
/// the caller's boundary description, not by the codec.
pub fn encode_cursor(
    sorts: &[(String, SortDirection)],
    values: &[serde_json::Value],
    id: &str,
) -> Result<String, CursorError> {
    if sorts.len() != values.len() {
        return Err(CursorError::Inconsistent(format!(
            "{} sort columns but {} boundary values",
            sorts.len(),
            values.len()
        )));
    }
    let payload = CursorPayload {
        v: CURSOR_VERSION,
        f: sorts.iter().map(|(f, _)| f.clone()).collect(),
        d: sorts
            .iter()
            .map(|(_, d)| if *d == SortDirection::Desc { 1 } else { 0 })
            .collect(),
        k: values.iter().map(value_as_text).collect(),
        id: id.to_string(),
    };
    let json = serde_json::to_vec(&payload)
        .map_err(|e| CursorError::Malformed(format!("cannot serialize: {e}")))?;
    Ok(base64url_encode(&json))
}

/// Decode a cursor, proving it matches the sort it will be applied to.
pub fn decode_cursor(
    opaque: &str,
    sorts: &[(String, SortDirection)],
) -> Result<CursorPayload, CursorError> {
    let bytes = base64url_decode(opaque)
        .map_err(|e| CursorError::Malformed(format!("cannot decode: {e}")))?;
    let payload: CursorPayload = serde_json::from_slice(&bytes)
        .map_err(|e| CursorError::Malformed(format!("cannot parse: {e}")))?;
    if payload.v != CURSOR_VERSION {
        return Err(CursorError::Version(payload.v));
    }
    if payload.f.len() != payload.d.len() || payload.f.len() != payload.k.len() {
        return Err(CursorError::Inconsistent(
            "field, direction and value arrays disagree in length".into(),
        ));
    }
    let expected: Vec<String> = sorts.iter().map(|(f, _)| f.clone()).collect();
    if payload.f != expected {
        return Err(CursorError::SortMismatch { expected: expected.join(", ") });
    }
    let expected_dirs: Vec<u8> = sorts
        .iter()
        .map(|(_, d)| if *d == SortDirection::Desc { 1 } else { 0 })
        .collect();
    if payload.d != expected_dirs {
        return Err(CursorError::SortMismatch { expected: expected.join(", ") });
    }
    Ok(payload)
}

/// The keyset predicate and its bound values, continuing the parameter
/// numbering at `param_idx`.
///
/// `casts` parallels the sort fields: the SQL type suffix each placeholder
/// is cast to (e.g. `numeric`, `timestamptz`), or None for a bare bind. The
/// direction of each comparison follows the sort direction for a forward
/// (`after`) walk, and is inverted for `before` — the caller inverts the
/// ORDER BY and reverses the rows to match.
pub fn build_keyset_predicate(
    payload: &CursorPayload,
    param_idx: &mut usize,
    casts: &[Option<String>],
    backwards: bool,
) -> (String, Vec<String>) {
    let n = payload.f.len();
    let mut params = Vec::with_capacity(n + 1);
    let mut terms = Vec::with_capacity(n + 1);

    // Prefix the column comparisons with casts decided by the caller; the
    // tiebreaker id is always a uuid.
    let cast = |i: usize| -> String {
        match casts.get(i).and_then(|c| c.clone()) {
            Some(t) => format!("::{t}"),
            None => String::new(),
        }
    };

    // Each term: everything before column i is equal, column i compares.
    for i in 0..n {
        let mut term_parts: Vec<String> = Vec::with_capacity(i + 1);
        for j in 0..i {
            term_parts.push(format!("{} = ${}{}", payload.f[j], *param_idx, cast(j)));
            params.push(payload.k[j].clone());
            *param_idx += 1;
        }
        let strict = if (payload.d[i] == 0) != backwards {
            ">"
        } else {
            "<"
        };
        term_parts.push(format!("{} {} ${}{}", payload.f[i], strict, *param_idx, cast(i)));
        params.push(payload.k[i].clone());
        *param_idx += 1;
        terms.push(format!("({})", term_parts.join(" AND ")));
    }
    // The final term: all sort columns equal, the tiebreaker decides —
    // UNLESS the caller's sort already ends on id, in which case the term
    // above already decides on it and appending a second id comparison
    // would compare the same column twice.
    if payload.f.last().map(|f| f != "id").unwrap_or(true) {
        let mut term_parts: Vec<String> = Vec::with_capacity(n + 1);
        for j in 0..n {
            term_parts.push(format!("{} = ${}{}", payload.f[j], *param_idx, cast(j)));
            params.push(payload.k[j].clone());
            *param_idx += 1;
        }
        let tiebreak = if !backwards { ">" } else { "<" };
        term_parts.push(format!("id {} ${}::uuid", tiebreak, *param_idx));
        params.push(payload.id.clone());
        *param_idx += 1;
        terms.push(format!("({})", term_parts.join(" AND ")));
    }

    (terms.join(" OR "), params)
}

/// A boundary row's value as cursor text: numbers and booleans keep their
/// JSON spelling, everything else renders through its string form. NULL is
/// refused by the caller (a NULL sort value cannot key a cursor position).
fn value_as_text(v: &serde_json::Value) -> String {
    match v {
        serde_json::Value::String(s) => s.clone(),
        other => other.to_string(),
    }
}

// ── base64url, no padding — the cursor must survive a query string ─────────

const B64_ALPHABET: &[u8; 64] =
    b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";

fn base64url_encode(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let b = [chunk[0], *chunk.get(1).unwrap_or(&0), *chunk.get(2).unwrap_or(&0)];
        let n = ((b[0] as u32) << 16) | ((b[1] as u32) << 8) | b[2] as u32;
        out.push(B64_ALPHABET[(n >> 18) as usize & 63] as char);
        out.push(B64_ALPHABET[(n >> 12) as usize & 63] as char);
        if chunk.len() > 1 {
            out.push(B64_ALPHABET[(n >> 6) as usize & 63] as char);
        }
        if chunk.len() > 2 {
            out.push(B64_ALPHABET[n as usize & 63] as char);
        }
    }
    out
}

fn base64url_decode(text: &str) -> Result<Vec<u8>, String> {
    let mut out = Vec::with_capacity(text.len() * 3 / 4);
    let mut acc: u32 = 0;
    let mut bits = 0u32;
    for c in text.chars() {
        let v = B64_ALPHABET
            .iter()
            .position(|&a| a as char == c)
            .ok_or_else(|| format!("character {c:?} is not base64url"))? as u32;
        acc = (acc << 6) | v;
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push((acc >> bits) as u8);
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sorts() -> Vec<(String, SortDirection)> {
        vec![
            ("created_at".into(), SortDirection::Desc),
            ("id".into(), SortDirection::Asc),
        ]
    }

    #[test]
    fn a_cursor_round_trips_through_its_opaque_form() {
        let c = encode_cursor(
            &sorts(),
            &[
                serde_json::json!("2026-09-19T01:02:03Z"),
                serde_json::json!("a0f0c0d0-0000-4000-8000-000000000001"),
            ],
            "a0f0c0d0-0000-4000-8000-000000000001",
        )
        .unwrap();
        // Opaque: no padding, url-safe alphabet only.
        assert!(!c.contains('='), "no padding in a query-string cursor: {c}");
        let p = decode_cursor(&c, &sorts()).unwrap();
        assert_eq!(p.k[0], "2026-09-19T01:02:03Z");
        assert_eq!(p.d, vec![1, 0]);
    }

    #[test]
    fn a_cursor_refuses_a_different_sort() {
        let c = encode_cursor(&sorts(), &[json!("x"), json!("y")], "id").unwrap();
        let wrong = vec![("created_at".into(), SortDirection::Asc)];
        assert!(matches!(
            decode_cursor(&c, &wrong),
            Err(CursorError::SortMismatch { .. })
        ));
    }

    #[test]
    fn garbage_is_refused_not_guessed_at() {
        assert!(matches!(
            decode_cursor("!!not-base64!!", &sorts()),
            Err(CursorError::Malformed(_))
        ));
        assert!(matches!(
            decode_cursor("aGVsbG8", &sorts()), // "hello" — valid b64, not JSON
            Err(CursorError::Malformed(_))
        ));
    }

    #[test]
    fn the_predicate_expands_mixed_directions_correctly() {
        let p = CursorPayload {
            v: 1,
            f: vec!["created_at".into(), "id".into()],
            d: vec![1, 0],
            k: vec!["2026-09-19T00:00:00Z".into(), "abc".into()],
            id: "d0d0".into(),
        };
        let mut idx = 3usize; // continue after two filter params
        let (sql, params) = build_keyset_predicate(
            &p,
            &mut idx,
            &[Some("timestamptz".into()), None],
            false,
        );
        // desc column compares with <, the asc id column with >; the sort
        // already ends on id, so no second id tiebreaker is appended.
        assert!(
            sql.contains("(created_at < $3::timestamptz)"),
            "{sql}"
        );
        assert!(
            sql.contains("(created_at = $4::timestamptz AND id > $5)"),
            "{sql}"
        );
        assert!(!sql.contains("id = $"), "no duplicated id comparison: {sql}");
        assert_eq!(params.len(), 3);
        // the id value rides k[1] (it IS a sort column here), not the tiebreaker
        assert_eq!(params[2], "abc");
    }

    #[test]
    fn a_backwards_walk_inverts_every_comparison() {
        let p = CursorPayload {
            v: 1,
            f: vec!["seq".into()],
            d: vec![0],
            k: vec!["41".into()],
            id: "x".into(),
        };
        let mut idx = 1usize;
        let (sql, _) = build_keyset_predicate(&p, &mut idx, &[Some("integer".into())], true);
        assert!(sql.starts_with("(seq < $1::integer)"), "{sql}");
        assert!(sql.contains("(seq = $2::integer AND id < $3::uuid)"), "{sql}");
    }
}
