//! Contract tests for the row-level tenant fence.
//!
//! These are pure — no database. The fence's job is to decide, from an entity's generated
//! metadata plus the request's scope, whether a query may run at all and what predicate it
//! carries. That decision is the security boundary, so it is tested directly rather than
//! inferred from an integration hit.
//!
//! TF-1  a global entity (no tenant column) is not fenced — reference data still reads
//! TF-2  a fenced entity with NO scope fails CLOSED (never an unfenced query)
//! TF-3  a fenced entity with a scope yields the ANDed predicate
//! TF-4  a client-supplied tenant filter is stripped (snake, camel, operator forms)
//! TF-5  a global entity's filters are untouched
//! TF-6  conditions compose so the fence ANDs with the soft-delete guard

use std::collections::HashMap;

use backbone_orm::{and_conditions, strip_client_company_filters, company_fence, EntityRepoMeta};
use uuid::Uuid;

/// A tenant-scoped entity — what the generator emits for any model carrying `company_id`.
struct Fenced;
impl EntityRepoMeta for Fenced {
    fn column_types() -> HashMap<String, String> {
        HashMap::new()
    }
    fn search_fields() -> &'static [&'static str] {
        &[]
    }
    fn company_field() -> Option<&'static str> {
        Some("company_id")
    }
}

/// A global entity — reference data (currencies, tax codes), or a model marked `@global`.
struct Global;
impl EntityRepoMeta for Global {
    fn column_types() -> HashMap<String, String> {
        HashMap::new()
    }
    fn search_fields() -> &'static [&'static str] {
        &[]
    }
}

#[test]
fn tf1_global_entity_is_not_fenced() {
    assert!(company_fence::<Global>(None).unwrap().is_none());
    assert!(company_fence::<Global>(Some(Uuid::new_v4())).unwrap().is_none());
}

#[test]
fn tf2_fenced_entity_without_scope_fails_closed() {
    // The whole point. Before this, an unscoped read returned every tenant's rows; it must
    // now be impossible to build the query at all.
    let err = company_fence::<Fenced>(None).expect_err("must not produce an unfenced query");
    assert_eq!(err.column, "company_id");
}

#[test]
fn tf3_fenced_entity_with_scope_yields_the_predicate() {
    let id = Uuid::new_v4();
    let cond = company_fence::<Fenced>(Some(id)).unwrap().expect("fenced");
    assert_eq!(cond, format!("company_id = '{id}'"));
    // A Uuid can only render hex and dashes, so the literal cannot carry a quote.
    assert!(!cond.contains('"'));
    assert_eq!(cond.matches('\'').count(), 2);
}

#[test]
fn tf4_client_supplied_tenant_filters_are_stripped() {
    let victim = Uuid::new_v4().to_string();
    let mut filters = HashMap::from([
        ("company_id".to_string(), victim.clone()),
        ("companyId".to_string(), victim.clone()),
        ("company_id[eq]".to_string(), victim.clone()),
        ("companyId[ne]".to_string(), victim),
        ("status".to_string(), "draft".to_string()),
        ("customer_id".to_string(), Uuid::new_v4().to_string()),
    ]);

    strip_client_company_filters::<Fenced>(&mut filters);

    assert!(
        !filters.keys().any(|k| k.to_lowercase().starts_with("company")),
        "no client-supplied tenant key may survive: {filters:?}"
    );
    // Narrowing within the tenant is still allowed — only the tenant itself is off-limits.
    assert_eq!(filters.get("status").map(String::as_str), Some("draft"));
    assert!(filters.contains_key("customer_id"));
}

#[test]
fn tf5_global_entity_filters_are_untouched() {
    let mut filters = HashMap::from([("company_id".to_string(), "anything".to_string())]);
    strip_client_company_filters::<Global>(&mut filters);
    // `Global` has no tenant column, so `company_id` here is an ordinary field, not a fence
    // to defend — stripping it would silently break a legitimate query.
    assert_eq!(filters.len(), 1);
}

#[test]
fn tf6_fence_ands_with_the_soft_delete_guard() {
    let id = Uuid::new_v4();
    let fence = company_fence::<Fenced>(Some(id)).unwrap();
    let combined = and_conditions(Some("metadata->>'deleted_at' IS NULL"), fence)
        .expect("both present");
    assert_eq!(
        combined,
        format!("metadata->>'deleted_at' IS NULL AND company_id = '{id}'")
    );

    // And each side survives alone.
    assert_eq!(
        and_conditions(Some("a = 1"), None).as_deref(),
        Some("a = 1")
    );
    assert_eq!(
        and_conditions(None, Some("b = 2".into())).as_deref(),
        Some("b = 2")
    );
    assert!(and_conditions(None, None).is_none());
}
