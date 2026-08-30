//! Unit tests for `?include=` relation-table qualification.

use crate::qualify_relation_table;

#[test]
fn bare_target_takes_the_callers_schema() {
    assert_eq!(
        qualify_relation_table("geo.provinces", "countries"),
        "geo.countries"
    );
}

#[test]
fn qualified_target_passes_through() {
    assert_eq!(
        qualify_relation_table("geo.provinces", "public.users"),
        "public.users"
    );
}

#[test]
fn bare_caller_leaves_the_target_bare() {
    // Entities whose own table carries no schema have nothing to qualify with;
    // the target resolves exactly as before.
    assert_eq!(qualify_relation_table("provinces", "countries"), "countries");
}

#[test]
fn caller_schema_is_only_the_first_segment() {
    // Defensive shape: a caller name that ever carries more than one dot
    // still contributes only its leading schema.
    assert_eq!(
        qualify_relation_table("edge.case.provinces", "countries"),
        "edge.countries"
    );
}
