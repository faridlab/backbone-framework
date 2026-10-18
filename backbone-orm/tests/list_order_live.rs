//! Live proof that every paged read has a deterministic total order.
//!
//! Gated on `BACKBONE_ORM_RLS_DSN` (a superuser DSN) — the tests create a schema. When it is unset
//! they skip, so a checkout without a database still passes.
//!
//! The defect these guard against: a list with no `sort` had no `ORDER BY`, so Postgres returned
//! rows in physical order, which changes after any update or vacuum — and offset paging over such a
//! list repeated a row on one page and skipped another between two fetches. A sorted list whose
//! sort keys tie had the same problem among the tied rows. Every paged read now ends its order on
//! the primary key.

use backbone_orm::generic_repository::{GenericCrudRepository, SoftDelete};
use backbone_orm::repository::PaginationParams;
use backbone_orm::EntityRepoMeta;
use sqlx::postgres::PgPoolOptions;
use sqlx::PgPool;
use std::collections::{HashMap, HashSet};
use uuid::Uuid;

fn admin_dsn() -> Option<String> {
    std::env::var("BACKBONE_ORM_RLS_DSN").ok()
}

#[derive(Debug, Clone, sqlx::FromRow)]
#[allow(dead_code)]
struct Item {
    id: Uuid,
    name: String,
    rank: i32,
    tier: String,
}

impl EntityRepoMeta for Item {
    fn column_types() -> HashMap<String, String> {
        HashMap::new()
    }
    fn search_fields() -> &'static [&'static str] {
        &["name"]
    }
}

type Repo = GenericCrudRepository<Item, SoftDelete>;

const ROWS: i32 = 60;

/// 60 rows with random ids; `rank` takes only 5 values (heavy ties), `tier` alternates, and every
/// fourth row is soft-deleted.
async fn setup(pool: &PgPool, schema: &str) {
    sqlx::raw_sql(&format!(
        "DROP SCHEMA IF EXISTS {schema} CASCADE;
         CREATE SCHEMA {schema};
         CREATE TYPE {schema}.item_state AS ENUM ('open', 'draft', 'done');
         CREATE TABLE {schema}.item (
             id       uuid PRIMARY KEY DEFAULT gen_random_uuid(),
             name     text NOT NULL,
             rank     integer NOT NULL,
             tier     text NOT NULL,
             state    {schema}.item_state NOT NULL DEFAULT 'open',
             seen_at  timestamp NOT NULL DEFAULT now(),
             metadata jsonb NOT NULL DEFAULT '{{}}'
         );
         INSERT INTO {schema}.item (name, rank, tier, state, seen_at, metadata)
         SELECT 'item ' || n, n % 5, CASE WHEN n % 2 = 0 THEN 'a' ELSE 'b' END,
                (ARRAY['open', 'draft', 'done'])[n % 3 + 1]::{schema}.item_state,
                timestamp '2026-01-01 00:00:00.123456' + ((n % 4) || ' minutes')::interval,
                CASE WHEN n % 4 = 0
                     THEN jsonb_build_object('deleted_at', now() - (n || ' minutes')::interval)
                     ELSE '{{}}'::jsonb END
           FROM generate_series(1, {ROWS}) AS n;"
    ))
    .execute(pool)
    .await
    .unwrap();
}

async fn pool() -> Option<(PgPool, String)> {
    let dsn = admin_dsn().or_else(|| {
        eprintln!("skipping: BACKBONE_ORM_RLS_DSN not set");
        None
    })?;
    let pool = PgPoolOptions::new()
        .max_connections(2)
        .connect(&dsn)
        .await
        .expect("connect");
    let schema = format!("orm_order_{}", &Uuid::new_v4().simple().to_string()[..8]);
    setup(&pool, &schema).await;
    Some((pool, schema))
}

async fn drop_schema(pool: &PgPool, schema: &str) {
    sqlx::raw_sql(&format!("DROP SCHEMA {schema} CASCADE"))
        .execute(pool)
        .await
        .unwrap();
}

fn params(pairs: &[(&str, &str)]) -> HashMap<String, String> {
    pairs
        .iter()
        .map(|(k, v)| (k.to_string(), v.to_string()))
        .collect()
}

/// The ids the database says a request should return, in order — the oracle.
async fn expected_ids(pool: &PgPool, sql: &str) -> Vec<Uuid> {
    sqlx::query_scalar::<_, Uuid>(sql)
        .fetch_all(pool)
        .await
        .unwrap()
}

/// Walk a whole list by offset, `per_page` rows at a time, and return the ids in the order they
/// arrived. `between` runs after each page is fetched — a place to disturb the table.
async fn walk_pages<F, Fut>(
    repo: &Repo,
    base: Option<&str>,
    filters: &HashMap<String, String>,
    per_page: u32,
    mut between: F,
) -> Vec<Uuid>
where
    F: FnMut(u32) -> Fut,
    Fut: std::future::Future<Output = ()>,
{
    let mut ids = Vec::new();
    let mut page = 1;
    loop {
        let got = repo
            .run_filtered_query(
                PaginationParams::new(page, per_page),
                base,
                filters,
                &HashMap::new(),
                &["name"],
            )
            .await
            .unwrap();
        let more = got.pagination.has_more == Some(true);
        ids.extend(got.data.iter().map(|i| i.id));
        between(page).await;
        if !more {
            return ids;
        }
        page += 1;
        assert!(page < 100, "paging never ended");
    }
}

/// Walk a whole list by keyset (`after=` the previous page's `next_cursor`).
async fn walk_cursor(
    repo: &Repo,
    base: Option<&str>,
    filters: &HashMap<String, String>,
    per_page: u32,
) -> Vec<Uuid> {
    let mut ids = Vec::new();
    let mut filters = filters.clone();
    for _ in 0..100 {
        let got = repo
            .run_filtered_query(
                PaginationParams::new(1, per_page),
                base,
                &filters,
                &HashMap::new(),
                &["name"],
            )
            .await
            .unwrap();
        ids.extend(got.data.iter().map(|i| i.id));
        match (got.pagination.has_more, got.pagination.next_cursor) {
            (Some(true), Some(cursor)) => {
                filters.insert("after".into(), cursor);
            }
            (Some(true), None) => panic!("a page with more rows handed out no cursor"),
            _ => return ids,
        }
    }
    panic!("cursor walk never ended");
}

fn assert_exactly_once(ids: &[Uuid], what: &str) {
    let unique: HashSet<_> = ids.iter().collect();
    assert_eq!(unique.len(), ids.len(), "{what}: a row was returned twice");
}

const ACTIVE: &str = "metadata->>'deleted_at' IS NULL";
const TRASH: &str = "metadata->>'deleted_at' IS NOT NULL";

#[tokio::test]
async fn an_unsorted_list_pages_in_key_order_without_repeats_or_gaps() {
    let Some((pool, schema)) = pool().await else {
        return;
    };
    let repo: Repo = Repo::new(pool.clone(), &format!("{schema}.item"));

    let want = expected_ids(
        &pool,
        &format!("SELECT id FROM {schema}.item WHERE {ACTIVE} ORDER BY id"),
    )
    .await;
    let got = walk_pages(&repo, Some(ACTIVE), &HashMap::new(), 7, |_| async {}).await;
    assert_exactly_once(&got, "unsorted");
    assert_eq!(got, want, "an unsorted list is ordered by id ascending");

    // The same walk by keyset arrives at the same rows in the same order.
    let by_cursor = walk_cursor(&repo, Some(ACTIVE), &HashMap::new(), 7).await;
    assert_eq!(by_cursor, want);

    drop_schema(&pool, &schema).await;
}

#[tokio::test]
async fn rows_that_tie_on_the_sort_key_still_page_in_a_fixed_order() {
    let Some((pool, schema)) = pool().await else {
        return;
    };
    let repo: Repo = Repo::new(pool.clone(), &format!("{schema}.item"));

    for (sort, order) in [
        ("rank", "rank, id"),
        ("-rank", "rank DESC, id"),
        ("tier,-rank", "tier, rank DESC, id"),
    ] {
        let want = expected_ids(
            &pool,
            &format!("SELECT id FROM {schema}.item WHERE {ACTIVE} ORDER BY {order}"),
        )
        .await;
        let filters = params(&[("sort", sort)]);
        let got = walk_pages(&repo, Some(ACTIVE), &filters, 7, |_| async {}).await;
        assert_exactly_once(&got, sort);
        assert_eq!(got, want, "sort={sort}");
        assert_eq!(
            walk_cursor(&repo, Some(ACTIVE), &filters, 7).await,
            want,
            "cursor walk, sort={sort}"
        );
    }

    // A caller who already sorts by the key keeps exactly the order asked for.
    let want = expected_ids(
        &pool,
        &format!("SELECT id FROM {schema}.item WHERE {ACTIVE} ORDER BY id DESC"),
    )
    .await;
    let got = walk_pages(
        &repo,
        Some(ACTIVE),
        &params(&[("sort", "-id")]),
        7,
        |_| async {},
    )
    .await;
    assert_eq!(got, want);

    drop_schema(&pool, &schema).await;
}

#[tokio::test]
async fn a_filtered_list_pages_in_key_order() {
    let Some((pool, schema)) = pool().await else {
        return;
    };
    let repo: Repo = Repo::new(pool.clone(), &format!("{schema}.item"));

    let filters = params(&[("tier", "a"), ("rank[gte]", "1")]);
    let want = expected_ids(
        &pool,
        &format!(
            "SELECT id FROM {schema}.item WHERE {ACTIVE} AND tier = 'a' AND rank >= 1 ORDER BY id"
        ),
    )
    .await;
    let got = walk_pages(&repo, Some(ACTIVE), &filters, 4, |_| async {}).await;
    assert_exactly_once(&got, "filtered");
    assert_eq!(got, want);
    assert_eq!(walk_cursor(&repo, Some(ACTIVE), &filters, 4).await, want);

    // A text search is a filter too.
    let searched = params(&[("search", "item 1")]);
    let got = walk_pages(&repo, Some(ACTIVE), &searched, 3, |_| async {}).await;
    assert_exactly_once(&got, "searched");
    let mut sorted = got.clone();
    sorted.sort();
    assert_eq!(got, sorted, "a search with no sort is ordered by id");

    drop_schema(&pool, &schema).await;
}

#[tokio::test]
async fn the_trash_list_pages_in_key_order_with_and_without_a_sort() {
    let Some((pool, schema)) = pool().await else {
        return;
    };
    let repo: Repo = Repo::new(pool.clone(), &format!("{schema}.item"));

    for (sort, order) in [(None, "id"), (Some("-rank"), "rank DESC, id")] {
        let want = expected_ids(
            &pool,
            &format!("SELECT id FROM {schema}.item WHERE {TRASH} ORDER BY {order}"),
        )
        .await;
        let filters = match sort {
            Some(s) => params(&[("sort", s)]),
            None => HashMap::new(),
        };
        let mut got = Vec::new();
        for page in 1..=10 {
            let r = repo
                .list_deleted_filtered(PaginationParams::new(page, 4), Some(&filters))
                .await
                .unwrap();
            got.extend(r.data.iter().map(|i| i.id));
            if r.pagination.has_more != Some(true) {
                break;
            }
        }
        assert_exactly_once(&got, "trash");
        assert_eq!(got, want, "trash sort={sort:?}");
    }

    // The unfiltered helpers carry the same rule: the key ascending, and the trash newest-deleted
    // first with the key breaking ties.
    let mut active = Vec::new();
    for page in 1..=10 {
        let r = repo
            .list_paginated(PaginationParams::new(page, 9))
            .await
            .unwrap();
        active.extend(r.data.iter().map(|i| i.id));
    }
    assert_eq!(
        active,
        expected_ids(
            &pool,
            &format!("SELECT id FROM {schema}.item WHERE {ACTIVE} ORDER BY id")
        )
        .await
    );

    drop_schema(&pool, &schema).await;
}

#[tokio::test]
async fn updating_rows_between_page_fetches_does_not_reorder_the_list() {
    let Some((pool, schema)) = pool().await else {
        return;
    };
    let repo: Repo = Repo::new(pool.clone(), &format!("{schema}.item"));

    let want = expected_ids(
        &pool,
        &format!("SELECT id FROM {schema}.item WHERE {ACTIVE} ORDER BY id"),
    )
    .await;

    // Every page fetch is followed by an update of EVERY row, which moves each row's live tuple
    // to a new physical position — the disturbance that made an unordered scan repeat and skip.
    let touch = |_page: u32| {
        let pool = pool.clone();
        let schema = schema.clone();
        async move {
            sqlx::raw_sql(&format!("UPDATE {schema}.item SET name = name || '.'"))
                .execute(&pool)
                .await
                .unwrap();
        }
    };
    let got = walk_pages(&repo, Some(ACTIVE), &HashMap::new(), 5, touch).await;
    assert_exactly_once(&got, "unsorted, updated between pages");
    assert_eq!(got, want);

    // And the same for a sorted list whose keys tie.
    let filters = params(&[("sort", "rank")]);
    let want = expected_ids(
        &pool,
        &format!("SELECT id FROM {schema}.item WHERE {ACTIVE} ORDER BY rank, id"),
    )
    .await;
    let touch = |_page: u32| {
        let pool = pool.clone();
        let schema = schema.clone();
        async move {
            sqlx::raw_sql(&format!("UPDATE {schema}.item SET tier = tier"))
                .execute(&pool)
                .await
                .unwrap();
        }
    };
    let got = walk_pages(&repo, Some(ACTIVE), &filters, 5, touch).await;
    assert_exactly_once(&got, "sorted with ties, updated between pages");
    assert_eq!(got, want);

    drop_schema(&pool, &schema).await;
}

#[tokio::test]
async fn an_unsorted_list_is_stable_across_update_and_vacuum() {
    let Some((pool, schema)) = pool().await else {
        return;
    };
    let repo: Repo = Repo::new(pool.clone(), &format!("{schema}.item"));

    let first = walk_pages(&repo, Some(ACTIVE), &HashMap::new(), 100, |_| async {}).await;

    sqlx::raw_sql(&format!(
        "UPDATE {schema}.item SET rank = rank WHERE rank % 2 = 0"
    ))
    .execute(&pool)
    .await
    .unwrap();
    let after_update = walk_pages(&repo, Some(ACTIVE), &HashMap::new(), 100, |_| async {}).await;
    assert_eq!(after_update, first, "an update reordered the unsorted list");

    sqlx::raw_sql(&format!("VACUUM FULL {schema}.item"))
        .execute(&pool)
        .await
        .unwrap();
    let after_vacuum = walk_pages(&repo, Some(ACTIVE), &HashMap::new(), 100, |_| async {}).await;
    assert_eq!(after_vacuum, first, "a vacuum reordered the unsorted list");

    drop_schema(&pool, &schema).await;
}

#[tokio::test]
async fn an_unsorted_list_hands_out_a_cursor_that_a_later_request_continues_from() {
    let Some((pool, schema)) = pool().await else {
        return;
    };
    let repo: Repo = Repo::new(pool.clone(), &format!("{schema}.item"));

    let page = repo
        .run_filtered_query(
            PaginationParams::new(1, 10),
            Some(ACTIVE),
            &HashMap::new(),
            &HashMap::new(),
            &[],
        )
        .await
        .unwrap();
    let cursor = page
        .pagination
        .next_cursor
        .clone()
        .expect("an unsorted page with more rows hands out a cursor");
    let next = repo
        .run_filtered_query(
            PaginationParams::new(1, 10),
            Some(ACTIVE),
            &params(&[("after", &cursor)]),
            &HashMap::new(),
            &[],
        )
        .await
        .unwrap();
    let want = expected_ids(
        &pool,
        &format!("SELECT id FROM {schema}.item WHERE {ACTIVE} ORDER BY id OFFSET 10 LIMIT 10"),
    )
    .await;
    assert_eq!(next.data.iter().map(|i| i.id).collect::<Vec<_>>(), want);

    drop_schema(&pool, &schema).await;
}

#[tokio::test]
async fn a_keyset_walk_sorted_by_an_enum_or_a_timestamp_column_hands_out_a_cursor_on_every_page() {
    let Some((pool, schema)) = pool().await else {
        return;
    };
    let repo: Repo = Repo::new(pool.clone(), &format!("{schema}.item"));

    // `state` is an enum whose declared order is not alphabetical; `seen_at` is a timestamp
    // without a time zone. Both tie heavily, so the id tiebreaker carries the walk.
    for (sort, order) in [
        ("state", "state, id"),
        ("-seen_at", "seen_at DESC, id"),
        ("state,-seen_at", "state, seen_at DESC, id"),
    ] {
        let want = expected_ids(
            &pool,
            &format!("SELECT id FROM {schema}.item WHERE {ACTIVE} ORDER BY {order}"),
        )
        .await;
        let filters = params(&[("sort", sort)]);
        assert_eq!(
            walk_cursor(&repo, Some(ACTIVE), &filters, 7).await,
            want,
            "keyset walk, sort={sort}"
        );
        assert_eq!(
            walk_pages(&repo, Some(ACTIVE), &filters, 7, |_| async {}).await,
            want,
            "offset walk, sort={sort}"
        );
    }

    drop_schema(&pool, &schema).await;
}
