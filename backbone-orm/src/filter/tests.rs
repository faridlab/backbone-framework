use super::*;
use super::parser::{to_snake_case, is_custom_enum_type};
use std::collections::HashMap;
use std::collections::HashSet;

#[test]
fn test_parse_operator() {
    assert_eq!(FilterOperator::from_str("eq"), Some(FilterOperator::Equal));
    assert_eq!(FilterOperator::from_str("gt"), Some(FilterOperator::GreaterThan));
    assert_eq!(FilterOperator::from_str("contain"), Some(FilterOperator::Contains));
    assert_eq!(FilterOperator::from_str("unknown"), None);
}

#[test]
fn test_parse_simple_filters() {
    let mut params = HashMap::new();
    params.insert("username[eq]".to_string(), "john".to_string());
    params.insert("age[gt]".to_string(), "18".to_string());

    let filter = parse_filters(&params, &HashMap::new(), None).unwrap();

    assert_eq!(filter.conditions.len(), 2);
    // HashMap iteration order is non-deterministic, so check by field name
    let username_cond = filter.conditions.iter().find(|c| c.field == "username").unwrap();
    assert_eq!(username_cond.operator, FilterOperator::Equal);
    let age_cond = filter.conditions.iter().find(|c| c.field == "age").unwrap();
    assert_eq!(age_cond.operator, FilterOperator::GreaterThan);
}

#[test]
fn test_build_where_clause() {
    let mut filter = QueryFilter::new();
    filter.add_condition(FilterCondition::new(
        "username".to_string(),
        FilterOperator::Equal,
        FilterValue::Single("john".to_string())
    ));

    let (where_clause, params) = filter.build_where_clause();

    assert!(where_clause.contains("username"));
    assert!(where_clause.contains("="));
    assert_eq!(params, vec!["john"]);
}

#[test]
fn test_contains_operator() {
    let mut filter = QueryFilter::new();
    filter.add_condition(FilterCondition::new(
        "username".to_string(),
        FilterOperator::Contains,
        FilterValue::Single("john".to_string())
    ));

    let (where_clause, params) = filter.build_where_clause();

    assert!(where_clause.contains("ILIKE"));
    assert_eq!(params, vec!["%john%"]);
}

#[test]
fn test_in_operator() {
    let mut filter = QueryFilter::new();
    filter.add_condition(FilterCondition::new(
        "status".to_string(),
        FilterOperator::In,
        FilterValue::Multiple(vec!["active".to_string(), "pending".to_string()])
    ));

    let (where_clause, params) = filter.build_where_clause();

    assert!(where_clause.contains("IN"));
    assert_eq!(params.len(), 2);
}

#[test]
fn test_between_operator() {
    let mut filter = QueryFilter::new();
    filter.add_condition(FilterCondition::new(
        "age".to_string(),
        FilterOperator::Between,
        FilterValue::Multiple(vec!["18".to_string(), "65".to_string()])
    ));

    let (where_clause, params) = filter.build_where_clause();

    assert!(where_clause.contains("BETWEEN"));
    assert_eq!(params, vec!["18", "65"]);
}

#[test]
fn test_not_between_operator() {
    let mut filter = QueryFilter::new();
    filter.add_condition(FilterCondition::new(
        "id".to_string(),
        FilterOperator::NotBetween,
        FilterValue::Multiple(vec!["1".to_string(), "100".to_string()])
    ));

    let (where_clause, params) = filter.build_where_clause();

    assert!(where_clause.contains("NOT BETWEEN"));
    assert_eq!(params, vec!["1", "100"]);
}

#[test]
fn test_startswith_operator() {
    let mut filter = QueryFilter::new();
    filter.add_condition(FilterCondition::new(
        "username".to_string(),
        FilterOperator::StartsWith,
        FilterValue::Single("admin".to_string())
    ));

    let (where_clause, params) = filter.build_where_clause();

    assert!(where_clause.contains("ILIKE"));
    assert_eq!(params, vec!["admin%"]);
}

#[test]
fn test_endswith_operator() {
    let mut filter = QueryFilter::new();
    filter.add_condition(FilterCondition::new(
        "email".to_string(),
        FilterOperator::EndsWith,
        FilterValue::Single("@gmail.com".to_string())
    ));

    let (where_clause, params) = filter.build_where_clause();

    assert!(where_clause.contains("ILIKE"));
    assert_eq!(params, vec!["%@gmail.com"]);
}

#[test]
fn test_or_conditions() {
    let mut params = HashMap::new();
    params.insert("username[or]".to_string(), "admin".to_string());
    params.insert("username[or]".to_string(), "superadmin".to_string());

    let filter = parse_filters(&params, &HashMap::new(), None).unwrap();

    // Should have at least one OR condition (the second one overwrites in HashMap, but OR logic is handled)
    assert!(!filter.conditions.is_empty());
}

#[test]
fn test_search_functionality() {
    let mut filter = QueryFilter::new();
    filter.search = Some("john".to_string());
    filter.search_fields = vec!["username".to_string(), "email".to_string()];

    let (where_clause, params) = filter.build_where_clause();

    assert!(where_clause.contains("ILIKE"));
    assert!(where_clause.contains("username"));
    assert!(where_clause.contains("email"));
    assert_eq!(params.len(), 2); // One for each field
    assert!(params.iter().all(|p| p == "%john%"));
}

#[test]
fn test_field_validation() {
    let allowed_fields: HashSet<String> = ["username", "email", "status"]
        .iter()
        .map(|s| s.to_string())
        .collect();

    assert!(is_valid_field("username", &allowed_fields));
    assert!(is_valid_field("email", &allowed_fields));
    assert!(!is_valid_field("invalid_field", &allowed_fields));
}

#[test]
fn test_sanitize_field_name() {
    assert!(sanitize_field_name("valid_field").is_ok());
    assert!(sanitize_field_name("anotherValid123").is_ok());
    assert!(sanitize_field_name("").is_err());
    assert!(sanitize_field_name("field-with-dash").is_err());
    assert!(sanitize_field_name("field;drop table").is_err());
    assert!(sanitize_field_name("field'--").is_err());
}

#[test]
fn test_parse_filters_with_allowed_fields() {
    let mut params = HashMap::new();
    params.insert("username[contain]".to_string(), "john".to_string());
    params.insert("invalid_field[eq]".to_string(), "value".to_string());

    let allowed_fields: HashSet<String> = ["username", "email"]
        .iter()
        .map(|s| s.to_string())
        .collect();

    let filter = parse_filters(&params, &HashMap::new(), Some(&allowed_fields)).unwrap();

    // Only valid field should be included
    assert_eq!(filter.conditions.len(), 1);
    assert_eq!(filter.conditions[0].field, "username");
}

#[test]
fn test_order_by() {
    let mut params = HashMap::new();
    params.insert("orderby".to_string(), "name,-age".to_string());

    let filter = parse_filters(&params, &HashMap::new(), None).unwrap();

    assert_eq!(filter.sorts.len(), 2);
    assert_eq!(filter.sorts[0].field, "name");
    assert_eq!(filter.sorts[0].direction, SortDirection::Asc);
    assert_eq!(filter.sorts[1].field, "age");
    assert_eq!(filter.sorts[1].direction, SortDirection::Desc);
}

#[test]
fn test_null_operators() {
    let mut filter = QueryFilter::new();
    filter.add_condition(FilterCondition::new(
        "deleted_at".to_string(),
        FilterOperator::IsNull,
        FilterValue::Null
    ));

    let (where_clause, params) = filter.build_where_clause();

    assert!(where_clause.contains("IS NULL"));
    assert!(params.is_empty());

    // Test IsNotNull
    let mut filter2 = QueryFilter::new();
    filter2.add_condition(FilterCondition::new(
        "verified_at".to_string(),
        FilterOperator::IsNotNull,
        FilterValue::Null
    ));

    let (where_clause2, params2) = filter2.build_where_clause();

    assert!(where_clause2.contains("IS NOT NULL"));
    assert!(params2.is_empty());
}

#[test]
fn test_comparison_operators() {
    let operators = vec![
        ("gt", FilterOperator::GreaterThan),
        ("gte", FilterOperator::GreaterThanOrEqual),
        ("lt", FilterOperator::LessThan),
        ("lte", FilterOperator::LessThanOrEqual),
    ];

    for (str_op, op) in operators {
        assert_eq!(FilterOperator::from_str(str_op), Some(op));
    }
}

#[test]
fn test_column_type_casting() {
    let condition = FilterCondition::new(
        "status".to_string(),
        FilterOperator::Equal,
        FilterValue::Single("active".to_string())
    ).with_column_type("user_status".to_string());

    let mut param_idx = 1;
    let sql = condition.build_sql_without_prefix(&mut param_idx);

    assert!(sql.contains("::user_status"));
}

#[test]
fn test_filterable_entity_trait() {
    struct TestEntity;
    impl FilterableEntity for TestEntity {
        fn filterable_fields() -> HashSet<String> {
            ["id", "name", "email", "status"]
                .iter().map(|s| s.to_string()).collect()
        }
    }

    let fields = TestEntity::filterable_fields();
    assert!(fields.contains("id"));
    assert!(fields.contains("name"));
    assert!(!fields.contains("password_hash"));

    // sortable_fields defaults to filterable_fields
    let sort_fields = TestEntity::sortable_fields();
    assert_eq!(fields, sort_fields);
}

#[test]
fn test_whitelist_blocks_sensitive_fields() {
    let mut params = HashMap::new();
    params.insert("username[eq]".to_string(), "john".to_string());
    params.insert("password_hash[eq]".to_string(), "secret".to_string());
    params.insert("internal_notes[contain]".to_string(), "test".to_string());
    params.insert("email[eq]".to_string(), "john@test.com".to_string());

    let allowed: HashSet<String> = ["username", "email", "status"]
        .iter().map(|s| s.to_string()).collect();

    let filter = parse_filters(&params, &HashMap::new(), Some(&allowed)).unwrap();

    // Only username and email should pass
    assert_eq!(filter.conditions.len(), 2);
    let field_names: Vec<&str> = filter.conditions.iter().map(|c| c.field.as_str()).collect();
    assert!(field_names.contains(&"username"));
    assert!(field_names.contains(&"email"));
    assert!(!field_names.contains(&"password_hash"));
    assert!(!field_names.contains(&"internal_notes"));
}

#[test]
fn test_whitelist_none_allows_all() {
    let mut params = HashMap::new();
    params.insert("any_field[eq]".to_string(), "value".to_string());
    params.insert("another_field[eq]".to_string(), "value2".to_string());

    let filter = parse_filters(&params, &HashMap::new(), None).unwrap();
    assert_eq!(filter.conditions.len(), 2);
}

#[test]
fn test_whitelist_empty_set_blocks_all() {
    let mut params = HashMap::new();
    params.insert("username[eq]".to_string(), "john".to_string());

    let allowed: HashSet<String> = HashSet::new();
    let filter = parse_filters(&params, &HashMap::new(), Some(&allowed)).unwrap();
    assert_eq!(filter.conditions.len(), 0);
}

#[test]
fn test_whitelist_with_sort_fields() {
    let mut params = HashMap::new();
    params.insert("orderby".to_string(), "name,-secret_score".to_string());
    params.insert("name[eq]".to_string(), "test".to_string());

    let allowed: HashSet<String> = ["name", "email"]
        .iter().map(|s| s.to_string()).collect();

    let filter = parse_filters(&params, &HashMap::new(), Some(&allowed)).unwrap();

    // Filter condition should work
    assert_eq!(filter.conditions.len(), 1);
    assert_eq!(filter.conditions[0].field, "name");
}

// =========================================================================
// Enum value normalization tests
// =========================================================================

#[test]
fn test_to_snake_case() {
    assert_eq!(to_snake_case("Detergent"), "detergent");
    assert_eq!(to_snake_case("StainRemover"), "stain_remover");
    assert_eq!(to_snake_case("DryCleanChemical"), "dry_clean_chemical");
    assert_eq!(to_snake_case("detergent"), "detergent");
    assert_eq!(to_snake_case("stain_remover"), "stain_remover");
    assert_eq!(to_snake_case("ACTIVE"), "a_c_t_i_v_e"); // all-caps edge case
    assert_eq!(to_snake_case(""), "");
}

#[test]
fn test_is_custom_enum_type() {
    // Built-in types should NOT be treated as enums
    assert!(!is_custom_enum_type("uuid"));
    assert!(!is_custom_enum_type("numeric"));
    assert!(!is_custom_enum_type("timestamptz"));
    assert!(!is_custom_enum_type("boolean"));
    assert!(!is_custom_enum_type("jsonb"));
    assert!(!is_custom_enum_type("text"));

    // Custom enum types SHOULD be detected
    assert!(is_custom_enum_type("inventory_item_type"));
    assert!(is_custom_enum_type("inventory_unit"));
    assert!(is_custom_enum_type("user_status"));
    assert!(is_custom_enum_type("order_status"));
}

#[test]
fn test_enum_normalization_simple_equality() {
    let mut params = HashMap::new();
    params.insert("item_type".to_string(), "Detergent".to_string());

    let mut column_types = HashMap::new();
    column_types.insert("item_type".to_string(), "inventory_item_type".to_string());

    let filter = parse_filters(&params, &column_types, None).unwrap();

    assert_eq!(filter.conditions.len(), 1);
    let params = filter.conditions[0].get_params();
    assert_eq!(params, vec!["detergent"]);
}

#[test]
fn test_enum_normalization_bracket_notation() {
    let mut params = HashMap::new();
    params.insert("item_type[eq]".to_string(), "StainRemover".to_string());

    let mut column_types = HashMap::new();
    column_types.insert("item_type".to_string(), "inventory_item_type".to_string());

    let filter = parse_filters(&params, &column_types, None).unwrap();

    assert_eq!(filter.conditions.len(), 1);
    let params = filter.conditions[0].get_params();
    assert_eq!(params, vec!["stain_remover"]);
}

#[test]
fn test_enum_normalization_does_not_affect_builtin_types() {
    let mut params = HashMap::new();
    params.insert("provider_id".to_string(), "153662ad-11bc-47cc-b611-7ae89784b916".to_string());

    let mut column_types = HashMap::new();
    column_types.insert("provider_id".to_string(), "uuid".to_string());

    let filter = parse_filters(&params, &column_types, None).unwrap();

    assert_eq!(filter.conditions.len(), 1);
    let params = filter.conditions[0].get_params();
    assert_eq!(params, vec!["153662ad-11bc-47cc-b611-7ae89784b916"]);
}

#[test]
fn test_enum_normalization_already_snake_case() {
    let mut params = HashMap::new();
    params.insert("item_type".to_string(), "dry_clean_chemical".to_string());

    let mut column_types = HashMap::new();
    column_types.insert("item_type".to_string(), "inventory_item_type".to_string());

    let filter = parse_filters(&params, &column_types, None).unwrap();

    let params = filter.conditions[0].get_params();
    assert_eq!(params, vec!["dry_clean_chemical"]);
}

#[test]
fn test_audit_metadata_rewrite_bracket() {
    // ?updated_at[gte]=... must read from metadata JSONB, not a top-level column
    let mut params = HashMap::new();
    params.insert("updated_at[gte]".to_string(), "2026-01-01T00:00:00Z".to_string());

    let column_types = HashMap::new();
    let filter = parse_filters(&params, &column_types, None).unwrap();

    assert_eq!(filter.conditions.len(), 1);
    let (where_clause, _) = filter.build_where_clause();
    assert!(
        where_clause.contains("(metadata->>'updated_at')::timestamptz"),
        "expected metadata rewrite, got: {}", where_clause
    );
    assert!(!where_clause.contains(" updated_at "), "bare column leaked: {}", where_clause);
}

#[test]
fn test_audit_metadata_rewrite_simple_equality() {
    let mut params = HashMap::new();
    params.insert("created_at".to_string(), "2026-01-01T00:00:00Z".to_string());

    let column_types = HashMap::new();
    let filter = parse_filters(&params, &column_types, None).unwrap();

    let (where_clause, _) = filter.build_where_clause();
    assert!(where_clause.contains("(metadata->>'created_at')::timestamptz"));
}

#[test]
fn test_audit_metadata_rewrite_orderby() {
    // Bracket form: orderby[updated_at]=desc
    let mut params = HashMap::new();
    params.insert("updated_at[orderby]".to_string(), "desc".to_string());

    let column_types = HashMap::new();
    let filter = parse_filters(&params, &column_types, None).unwrap();

    assert_eq!(filter.sorts.len(), 1);
    assert_eq!(filter.sorts[0].field, "(metadata->>'updated_at')::timestamptz");

    // Multi-field form with DESC prefix: orderby=-updated_at,name
    let mut params2 = HashMap::new();
    params2.insert("orderby".to_string(), "-updated_at,name".to_string());
    let filter2 = parse_filters(&params2, &column_types, None).unwrap();
    assert_eq!(filter2.sorts.len(), 2);
    assert_eq!(filter2.sorts[0].field, "(metadata->>'updated_at')::timestamptz");
    assert_eq!(filter2.sorts[1].field, "name");
}

#[test]
fn test_audit_metadata_does_not_affect_other_fields() {
    let mut params = HashMap::new();
    params.insert("name".to_string(), "alice".to_string());

    let column_types = HashMap::new();
    let filter = parse_filters(&params, &column_types, None).unwrap();

    let (where_clause, _) = filter.build_where_clause();
    assert!(!where_clause.contains("metadata"));
    assert!(where_clause.contains("name"));
}
