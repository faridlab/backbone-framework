//! Criterion benchmarks for backbone-orm filter parsing and SQL construction
//!
//! Run with: `cargo bench -p backbone-orm --bench filter_bench`

use criterion::{black_box, criterion_group, criterion_main, BenchmarkId, Criterion, Throughput};
use std::collections::{HashMap, HashSet};

use backbone_orm::filter::{
    parse_filters, sanitize_field_name, FilterCondition,
    FilterOperator, FilterValue, QueryFilter, SortDirection, SortSpec,
};
use backbone_orm::query_builder::{QueryBuilder, QueryValue};
use backbone_orm::raw_query::JoinType;

// ============================================================================
// Helpers
// ============================================================================

fn make_column_types(n: usize) -> HashMap<String, String> {
    let mut types = HashMap::new();
    for i in 0..n {
        types.insert(format!("field_{}", i), "text".to_string());
    }
    types.insert("age".to_string(), "integer".to_string());
    types.insert("score".to_string(), "float".to_string());
    types.insert("name".to_string(), "text".to_string());
    types.insert("email".to_string(), "text".to_string());
    types.insert("status".to_string(), "text".to_string());
    types
}

fn make_filter_params(n: usize) -> HashMap<String, String> {
    let operators = ["eq", "gt", "lt", "contain", "like", "gte", "lte", "noteq", "ilike", "startwith"];
    let mut params = HashMap::new();
    for i in 0..n {
        let op = operators[i % operators.len()];
        params.insert(format!("field_{}[{}]", i, op), format!("value_{}", i));
    }
    params
}

fn make_query_filter(n_conditions: usize) -> QueryFilter {
    let mut filter = QueryFilter::new();
    for i in 0..n_conditions {
        filter.add_condition(FilterCondition::new(
            format!("field_{}", i),
            FilterOperator::Equal,
            FilterValue::Single(format!("value_{}", i)),
        ));
    }
    filter
}

fn make_query_filter_with_sorts(n_sorts: usize) -> QueryFilter {
    let mut filter = QueryFilter::new();
    for i in 0..n_sorts {
        filter.add_sort(SortSpec::new(
            format!("field_{}", i),
            if i % 2 == 0 { SortDirection::Asc } else { SortDirection::Desc },
        ));
    }
    filter
}

// ============================================================================
// Benchmarks
// ============================================================================

fn bench_sanitize_field_name(c: &mut Criterion) {
    let mut group = c.benchmark_group("sanitize_field_name");

    group.bench_function("valid_simple", |b| {
        b.iter(|| sanitize_field_name(black_box("username")))
    });

    group.bench_function("valid_with_underscore", |b| {
        b.iter(|| sanitize_field_name(black_box("first_name")))
    });

    group.bench_function("valid_long", |b| {
        b.iter(|| sanitize_field_name(black_box("very_long_field_name_with_many_underscores")))
    });

    group.bench_function("invalid_special_chars", |b| {
        b.iter(|| sanitize_field_name(black_box("field; DROP TABLE users")))
    });

    group.finish();
}

fn bench_filter_operator_from_str(c: &mut Criterion) {
    let operators = [
        "eq", "noteq", "gt", "gte", "lt", "lte",
        "like", "ilike", "contain", "in", "between",
    ];

    let mut group = c.benchmark_group("filter_operator_from_str");
    for op in &operators {
        group.bench_with_input(BenchmarkId::from_parameter(op), op, |b, op| {
            b.iter(|| FilterOperator::from_str(black_box(op)))
        });
    }
    group.finish();
}

fn bench_parse_filters(c: &mut Criterion) {
    let mut group = c.benchmark_group("parse_filters");
    let sizes: &[usize] = &[1, 5, 10, 20, 50];

    for &size in sizes {
        let params = make_filter_params(size);
        let column_types = make_column_types(size);

        group.throughput(Throughput::Elements(size as u64));
        group.bench_with_input(
            BenchmarkId::from_parameter(size),
            &(params, column_types),
            |b, (params, column_types)| {
                b.iter(|| parse_filters(black_box(params), black_box(column_types), None))
            },
        );
    }
    group.finish();
}

fn bench_parse_filters_with_whitelist(c: &mut Criterion) {
    let mut group = c.benchmark_group("parse_filters_whitelisted");
    let sizes: &[usize] = &[5, 10, 20];

    for &size in sizes {
        let params = make_filter_params(size);
        let column_types = make_column_types(size);
        let allowed: HashSet<String> = (0..size).map(|i| format!("field_{}", i)).collect();

        group.throughput(Throughput::Elements(size as u64));
        group.bench_with_input(
            BenchmarkId::from_parameter(size),
            &(params, column_types, allowed),
            |b, (params, column_types, allowed)| {
                b.iter(|| parse_filters(black_box(params), black_box(column_types), Some(black_box(allowed))))
            },
        );
    }
    group.finish();
}

fn bench_build_where_clause(c: &mut Criterion) {
    let mut group = c.benchmark_group("build_where_clause");
    let sizes: &[usize] = &[1, 5, 10, 20];

    for &size in sizes {
        let filter = make_query_filter(size);

        group.throughput(Throughput::Elements(size as u64));
        group.bench_with_input(
            BenchmarkId::from_parameter(size),
            &filter,
            |b, filter| {
                b.iter(|| black_box(filter).build_where_clause())
            },
        );
    }
    group.finish();
}

fn bench_build_order_by_clause(c: &mut Criterion) {
    let mut group = c.benchmark_group("build_order_by_clause");
    let sizes: &[usize] = &[1, 3, 5];

    for &size in sizes {
        let filter = make_query_filter_with_sorts(size);

        group.bench_with_input(
            BenchmarkId::from_parameter(size),
            &filter,
            |b, filter| {
                b.iter(|| black_box(filter).build_order_by_clause())
            },
        );
    }
    group.finish();
}

fn bench_query_builder_build_sql(c: &mut Criterion) {
    let mut group = c.benchmark_group("query_builder_build_sql");

    group.bench_function("simple_select", |b| {
        b.iter(|| {
            let qb = QueryBuilder::new("users")
                .select(&["id", "name", "email"]);
            black_box(qb.build_sql())
        })
    });

    group.bench_function("with_conditions", |b| {
        b.iter(|| {
            let qb = QueryBuilder::new("users")
                .select(&["id", "name", "email"])
                .where_eq("status", QueryValue::Text("active".into()))
                .where_gt("age", QueryValue::Integer(18))
                .where_like("name", QueryValue::Text("%john%".into()));
            black_box(qb.build_sql())
        })
    });

    group.bench_function("with_joins", |b| {
        b.iter(|| {
            let qb = QueryBuilder::new("orders")
                .select(&["orders.id", "users.name", "products.title"])
                .join(JoinType::Inner, "users", "users.id = orders.user_id")
                .join(JoinType::Left, "products", "products.id = orders.product_id")
                .where_eq("orders.status", QueryValue::Text("pending".into()))
                .order_by("orders.created_at", "DESC")
                .limit(20)
                .offset(0);
            black_box(qb.build_sql())
        })
    });

    group.bench_function("complex_query", |b| {
        b.iter(|| {
            let qb = QueryBuilder::new("orders")
                .select(&["orders.*", "users.name", "users.email", "p.title", "p.price"])
                .join(JoinType::Inner, "users", "users.id = orders.user_id")
                .join(JoinType::Left, "products", "products.id = orders.product_id")
                .join_alias(JoinType::Left, "product_categories", "pc", "pc.product_id = products.id")
                .where_eq("orders.status", QueryValue::Text("pending".into()))
                .where_gt("orders.total", QueryValue::Float(100.0))
                .where_in("orders.region", vec![
                    QueryValue::Text("US".into()),
                    QueryValue::Text("EU".into()),
                    QueryValue::Text("APAC".into()),
                ])
                .order_by("orders.created_at", "DESC")
                .order_by("orders.total", "ASC")
                .limit(50)
                .offset(100);
            black_box(qb.build_sql())
        })
    });

    group.finish();
}

criterion_group!(
    benches,
    bench_sanitize_field_name,
    bench_filter_operator_from_str,
    bench_parse_filters,
    bench_parse_filters_with_whitelist,
    bench_build_where_clause,
    bench_build_order_by_clause,
    bench_query_builder_build_sql,
);
criterion_main!(benches);
