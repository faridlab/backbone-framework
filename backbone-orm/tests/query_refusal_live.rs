//! Live proof that a list request's bad parameter is refused BY NAME, whichever layer finds it
//! out: the parser (an operator, a field name, a paging number), the repository (a cursor, an
//! aggregate column) or the database (a column the table lacks, a value the column cannot take).
//!
//! Gated on `BACKBONE_ORM_RLS_DSN` (a superuser DSN, e.g.
//! `postgresql://root:password@localhost:5434/postgres`) — the test creates a schema. When it is
//! unset the test skips, so a checkout without a database still passes.
//!
//! The defect these guard against: a bad `sort_by` / filter answered "Invalid query parameter or
//! filter" with no parameter named, and a few kinds answered the database's own sentence (SQL,
//! table and type names) or nothing at all.

use backbone_orm::repository::{AggregateFn, AggregateSpec, PaginationParams, PostgresRepository};
use backbone_orm::{QueryParamError, QueryParamReason};
use sqlx::postgres::PgPoolOptions;
use sqlx::PgPool;
use std::collections::HashMap;
use uuid::Uuid;

fn admin_dsn() -> Option<String> {
    std::env::var("BACKBONE_ORM_RLS_DSN").ok()
}

#[derive(Debug, Clone, sqlx::FromRow)]
#[allow(dead_code)]
struct Stage {
    id: Uuid,
    name: String,
}

async fn setup(admin: &PgPool, schema: &str) {
    sqlx::raw_sql(&format!(
        "DROP SCHEMA IF EXISTS {schema} CASCADE;
         CREATE SCHEMA {schema};
         CREATE TYPE {schema}.stage_kind AS ENUM ('open_call', 'closed');
         CREATE TABLE {schema}.stage (
             id            uuid PRIMARY KEY,
             name          text NOT NULL,
             folded        boolean NOT NULL,
             sequence      integer NOT NULL,
             salary        numeric(14,2) NOT NULL,
             requested_by  uuid NOT NULL,
             deadline      date NOT NULL,
             kind          {schema}.stage_kind NOT NULL
         );
         INSERT INTO {schema}.stage VALUES
           ('00000000-0000-0000-0000-000000000001', 'Screening', false, 10, 1000.50,
            '6f1c2a8e-0b7d-4e44-9a51-3c2d1e0f9a77', '2026-09-19', 'open_call'),
           ('00000000-0000-0000-0000-000000000002', 'Offer', true, 20, 2500.00,
            '6f1c2a8e-0b7d-4e44-9a51-3c2d1e0f9a77', '2026-10-03', 'closed');"
    ))
    .execute(admin)
    .await
    .unwrap();
}

fn map(pairs: &[(&str, &str)]) -> HashMap<String, String> {
    pairs
        .iter()
        .map(|(k, v)| (k.to_string(), v.to_string()))
        .collect()
}

async fn list(
    repo: &PostgresRepository<Stage>,
    filters: &[(&str, &str)],
) -> anyhow::Result<Vec<String>> {
    let page = repo
        .list_with_filters(
            PaginationParams::new(1, 50),
            &map(filters),
            &HashMap::new(),
            &[],
        )
        .await?;
    Ok(page.data.into_iter().map(|s| s.name).collect())
}

/// What a refused request names: the parameter and the reason, after asserting the sentence
/// names the parameter and carries nothing of the database.
fn refusal(
    what: &dyn std::fmt::Debug,
    result: anyhow::Result<impl std::fmt::Debug>,
    schema: &str,
) -> (String, QueryParamReason) {
    let err = result.expect_err(&format!("{what:?} should be refused"));
    let named = err
        .downcast_ref::<QueryParamError>()
        .unwrap_or_else(|| panic!("not a named refusal: {err:#}"));
    assert!(
        named.message.contains(&named.parameter),
        "the sentence names the parameter: {}",
        named.message
    );
    for leaked in [
        schema,
        "stage",
        "SELECT",
        "::",
        "pg_",
        "does not exist",
        "syntax",
    ] {
        // `stage` is the table; a client's own value could contain it, none of these do.
        assert!(
            !named.message.contains(leaked),
            "the sentence leaks `{leaked}`: {}",
            named.message
        );
    }
    (named.parameter.clone(), named.reason)
}

#[tokio::test]
async fn every_kind_of_bad_list_parameter_is_refused_naming_its_parameter() {
    let Some(dsn) = admin_dsn() else {
        eprintln!("skipping: BACKBONE_ORM_RLS_DSN not set");
        return;
    };
    let pool = PgPoolOptions::new()
        .max_connections(2)
        .connect(&dsn)
        .await
        .expect("connect");
    let schema = format!("orm_refuse_{}", &Uuid::new_v4().simple().to_string()[..8]);
    setup(&pool, &schema).await;
    let repo: PostgresRepository<Stage> =
        PostgresRepository::new(pool.clone(), &format!("{schema}.stage"));
    let s = schema.as_str();

    // ── A column the table does not have, in every grammar that can name one ──
    for (filters, parameter) in [
        (vec![("full_name", "x")], "full_name"),
        (vec![("fullName", "x")], "fullName"),
        (vec![("full_name[gte]", "x")], "full_name[gte]"),
        (vec![("full_name[in]", "a,b")], "full_name[in]"),
        (vec![("full_name[or]", "a")], "full_name[or]"),
        (vec![("orderby", "full_name")], "orderby"),
        (vec![("orderby", "name,-full_name")], "orderby"),
        (vec![("sort", "-full_name")], "sort"),
        (vec![("orderby[full_name]", "desc")], "orderby[full_name]"),
        (
            vec![("searchFields", "name,full_name"), ("search", "a")],
            "searchFields",
        ),
    ] {
        let (got, reason) = refusal(&filters, list(&repo, &filters).await, s);
        assert_eq!(
            (got.as_str(), reason),
            (parameter, QueryParamReason::UnknownColumn),
            "{filters:?}"
        );
    }
    // With both a good and a bad parameter, the bad one is the one named.
    let (got, _) = refusal(
        &"name+nope",
        list(&repo, &[("name", "Offer"), ("nope[eq]", "1")]).await,
        s,
    );
    assert_eq!(got, "nope[eq]");

    // ── A value the column cannot take ──
    for (filters, parameter) in [
        (vec![("sequence", "abc")], "sequence"),
        (vec![("sequence[gte]", "ten")], "sequence[gte]"),
        (vec![("sequence[in]", "1,abc,3")], "sequence[in]"),
        (vec![("sequence[between]", "1,two")], "sequence[between]"),
        (vec![("sequence", "99999999999")], "sequence"),
        (vec![("salary[lt]", "lots")], "salary[lt]"),
        (vec![("requested_by", "not-a-uuid")], "requested_by"),
        (vec![("folded", "maybe")], "folded"),
        (vec![("deadline", "2026-13-45")], "deadline"),
        (vec![("deadline[gte]", "not-a-date")], "deadline[gte]"),
        (vec![("kind", "bogus")], "kind"),
        (vec![("kind[in]", "closed,bogus")], "kind[in]"),
    ] {
        let (got, reason) = refusal(&filters, list(&repo, &filters).await, s);
        assert_eq!(
            (got.as_str(), reason),
            (parameter, QueryParamReason::InvalidValue),
            "{filters:?}"
        );
    }
    // The sentence says what the column takes, not what the database called it.
    let err = list(&repo, &[("kind", "bogus")]).await.unwrap_err();
    let msg = err
        .downcast_ref::<QueryParamError>()
        .unwrap()
        .message
        .clone();
    assert!(
        msg.contains("one of the accepted values") && !msg.contains("stage_kind"),
        "{msg}"
    );
    let err = list(&repo, &[("requested_by", "zz")]).await.unwrap_err();
    assert!(err
        .downcast_ref::<QueryParamError>()
        .unwrap()
        .message
        .contains("a UUID"));

    // ── The grammar's own refusals ──
    for (filters, parameter, want) in [
        (
            vec![("sequence[neq]", "3")],
            "sequence[neq]",
            QueryParamReason::UnknownOperator,
        ),
        (
            vec![("sequence[contain]", "1")],
            "sequence[contain]",
            QueryParamReason::UnsupportedOperator,
        ),
        // Two pattern filters: the catalog says which one is on a column that is not text.
        (
            vec![("name[startwith]", "S"), ("sequence[contain]", "1")],
            "sequence[contain]",
            QueryParamReason::UnsupportedOperator,
        ),
        (
            vec![("sequence[", "3")],
            "sequence[",
            QueryParamReason::InvalidFieldName,
        ),
        (
            vec![("na-me", "x")],
            "na-me",
            QueryParamReason::InvalidFieldName,
        ),
        (
            vec![("orderby", "na-me")],
            "orderby",
            QueryParamReason::InvalidFieldName,
        ),
        (
            vec![("offset", "abc")],
            "offset",
            QueryParamReason::NotANumber,
        ),
        (
            vec![("pageSize", "-4")],
            "pageSize",
            QueryParamReason::NotANumber,
        ),
    ] {
        let (got, reason) = refusal(&filters, list(&repo, &filters).await, s);
        assert_eq!((got.as_str(), reason), (parameter, want), "{filters:?}");
    }

    // ── A cursor this endpoint did not issue ──
    let (got, reason) = refusal(
        &"after",
        list(&repo, &[("orderby", "sequence"), ("after", "garbage")]).await,
        s,
    );
    assert_eq!(
        (got.as_str(), reason),
        ("after", QueryParamReason::InvalidCursor)
    );
    let (got, reason) = refusal(
        &"before",
        list(&repo, &[("orderby", "sequence"), ("before", "garbage")]).await,
        s,
    );
    assert_eq!(
        (got.as_str(), reason),
        ("before", QueryParamReason::InvalidCursor)
    );
    // A real cursor from another sort is refused too.
    let page = repo
        .list_with_filters(
            PaginationParams::new(1, 1),
            &map(&[("orderby", "sequence")]),
            &HashMap::new(),
            &[],
        )
        .await
        .unwrap();
    let cursor = page
        .pagination
        .next_cursor
        .expect("a sorted page hands out a cursor");
    let (got, reason) = refusal(
        &"cursor",
        list(&repo, &[("orderby", "name"), ("after", cursor.as_str())]).await,
        s,
    );
    assert_eq!(
        (got.as_str(), reason),
        ("after", QueryParamReason::InvalidCursor)
    );

    // ── Aggregates ──
    let agg = |group_by: Option<&str>, reductions: Vec<(AggregateFn, &str)>| AggregateSpec {
        group_by: group_by.map(str::to_string),
        reductions: reductions
            .into_iter()
            .map(|(f, c)| (f, c.to_string()))
            .collect(),
        group_limit: 0,
        label_field: None,
        label_relation: None,
    };
    for (spec, parameter, want) in [
        (
            agg(Some("full_name"), vec![]),
            "group_by",
            QueryParamReason::UnknownColumn,
        ),
        (
            agg(None, vec![(AggregateFn::Sum, "nope")]),
            "sum",
            QueryParamReason::UnknownColumn,
        ),
        (
            agg(None, vec![(AggregateFn::Avg, "nope")]),
            "avg",
            QueryParamReason::UnknownColumn,
        ),
        (
            agg(None, vec![(AggregateFn::Min, "nope")]),
            "min",
            QueryParamReason::UnknownColumn,
        ),
        (
            agg(None, vec![(AggregateFn::Max, "nope")]),
            "max",
            QueryParamReason::UnknownColumn,
        ),
        (
            agg(None, vec![(AggregateFn::Sum, "name")]),
            "sum",
            QueryParamReason::NotNumeric,
        ),
    ] {
        let result = repo
            .aggregate_with_filters(&spec, &HashMap::new(), &HashMap::new(), &[])
            .await;
        let (got, reason) = refusal(&parameter, result, s);
        assert_eq!((got.as_str(), reason), (parameter, want), "{parameter}");
    }
    // A filter on an aggregate is refused by name too.
    let result = repo
        .aggregate_with_filters(
            &agg(None, vec![]),
            &map(&[("nope", "1")]),
            &HashMap::new(),
            &[],
        )
        .await;
    assert_eq!(refusal(&"agg filter", result, s).0, "nope");

    // ── What is well formed is still served ──
    assert_eq!(
        list(&repo, &[("sequence[gte]", "15"), ("kind", "Closed")])
            .await
            .unwrap(),
        ["Offer"]
    );
    assert_eq!(
        list(
            &repo,
            &[("orderby", "-sequence"), ("limit", "1"), ("offset", "0")]
        )
        .await
        .unwrap()
        .len(),
        2
    );
    assert_eq!(
        list(&repo, &[("orderby", ""), ("limit", "")])
            .await
            .unwrap()
            .len(),
        2
    );

    sqlx::raw_sql(&format!("DROP SCHEMA {schema} CASCADE"))
        .execute(&pool)
        .await
        .unwrap();
}

/// The refusal reaches a caller that holds only a [`backbone_core`]-style repository error:
/// the typed error survives `anyhow` context wrapping, which is how the service layer sees it.
#[tokio::test]
async fn a_named_refusal_survives_context_wrapping() {
    let Some(dsn) = admin_dsn() else {
        eprintln!("skipping: BACKBONE_ORM_RLS_DSN not set");
        return;
    };
    let pool = PgPoolOptions::new()
        .max_connections(2)
        .connect(&dsn)
        .await
        .expect("connect");
    let schema = format!("orm_refuse_{}", &Uuid::new_v4().simple().to_string()[..8]);
    setup(&pool, &schema).await;
    let repo: PostgresRepository<Stage> =
        PostgresRepository::new(pool.clone(), &format!("{schema}.stage"));
    let err = list(&repo, &[("full_name", "x")])
        .await
        .unwrap_err()
        .context("while listing stages");
    assert_eq!(
        err.downcast_ref::<QueryParamError>().unwrap().parameter,
        "full_name"
    );
    sqlx::raw_sql(&format!("DROP SCHEMA {schema} CASCADE"))
        .execute(&pool)
        .await
        .unwrap();
}
