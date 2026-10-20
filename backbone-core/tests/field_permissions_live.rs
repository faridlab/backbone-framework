//! Live proof of the field-permission gate at the HTTP seam: a field whose entity declares
//! `field_permissions()` is redacted to `null` for a caller whose grants lack the permission
//! (the key stays), served to a holder and to `*:*`, refused as a column of a non-holder's
//! filter/sort/aggregate, and refused as a write that sets it — all against a scratch Postgres
//! schema the generic routes really serve.
//!
//! Gated on `BACKBONE_ORM_RLS_DSN` (a superuser DSN): the test creates a schema and drops it.
//! When the variable is unset the tests skip, so a checkout without a database still passes.

use std::collections::HashMap;
use std::sync::Arc;

use async_trait::async_trait;
use axum::body::Body;
use axum::http::{Request, StatusCode};
use backbone_auth::middleware::AuthContext;
use backbone_core::http::BackboneCrudHandler;
use backbone_core::service::{ApplyUpdateDto, FromCreateDto, GenericCrudService, ServiceResult};
use backbone_core::PersistentEntity;
use backbone_messaging::crud_event::{CrudEvent, CrudEventPublisher};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sqlx::postgres::PgPoolOptions;
use sqlx::PgPool;
use tower::ServiceExt;
use uuid::Uuid;

// ── The entity, its repository and service ───────────────────────────────────

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, sqlx::FromRow)]
struct Deal {
    id: Uuid,
    name: String,
    bid_value: Option<f64>,
    metadata: serde_json::Value,
}

/// The wire shape (camelCase), as a generated response DTO would be: the gate's keys are
/// response keys, so this is what carries `bidValue` through `secure`.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct DealResponse {
    id: Uuid,
    name: String,
    bid_value: Option<f64>,
    metadata: serde_json::Value,
}

impl From<Deal> for DealResponse {
    fn from(d: Deal) -> Self {
        DealResponse {
            id: d.id,
            name: d.name,
            bid_value: d.bid_value,
            metadata: d.metadata,
        }
    }
}

impl PersistentEntity for Deal {
    fn entity_id(&self) -> String {
        self.id.to_string()
    }
    fn set_entity_id(&mut self, id: String) {
        self.id = id.parse().unwrap();
    }
    fn created_at(&self) -> Option<DateTime<Utc>> {
        None
    }
    fn set_created_at(&mut self, _ts: DateTime<Utc>) {}
    fn updated_at(&self) -> Option<DateTime<Utc>> {
        None
    }
    fn set_updated_at(&mut self, _ts: DateTime<Utc>) {}
    fn deleted_at(&self) -> Option<DateTime<Utc>> {
        None
    }
    fn set_deleted_at(&mut self, _ts: Option<DateTime<Utc>>) {}
}

impl backbone_orm::EntityRepoMeta for Deal {
    fn column_types() -> HashMap<String, String> {
        HashMap::from([("id".to_string(), "uuid".to_string())])
    }
    fn search_fields() -> &'static [&'static str] {
        &["name"]
    }
    /// The gate under test: `bidValue` only a holder of `deal:read_bid` may see, ask about or set.
    fn field_permissions() -> &'static [(&'static str, &'static str)] {
        &[("bidValue", "deal:read_bid")]
    }
}

struct DealRepo(backbone_orm::GenericCrudRepository<Deal, backbone_orm::SoftDelete>);

impl std::ops::Deref for DealRepo {
    type Target = backbone_orm::GenericCrudRepository<Deal, backbone_orm::SoftDelete>;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

backbone_core::impl_crud_repository!(DealRepo, Deal, soft_delete);

#[derive(Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct CreateDeal {
    name: String,
    #[serde(default)]
    bid_value: Option<f64>,
}

#[derive(Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct UpdateDeal {
    name: String,
    #[serde(default)]
    bid_value: Option<f64>,
}

impl FromCreateDto<CreateDeal> for Deal {
    fn from_create_dto(dto: CreateDeal) -> ServiceResult<Self> {
        Ok(Deal {
            id: Uuid::new_v4(),
            name: dto.name,
            bid_value: dto.bid_value,
            metadata: serde_json::json!({}),
        })
    }
}

impl ApplyUpdateDto<UpdateDeal> for Deal {
    fn apply_update(mut self, dto: UpdateDeal) -> ServiceResult<Self> {
        self.name = dto.name;
        self.bid_value = dto.bid_value;
        Ok(self)
    }
}

type DealService = GenericCrudService<Deal, CreateDeal, UpdateDeal, DealRepo>;

struct QuietPublisher;

#[async_trait]
impl CrudEventPublisher<Deal> for QuietPublisher {
    async fn publish(&self, _event: CrudEvent<Deal>) -> Result<(), backbone_messaging::EventError> {
        Ok(())
    }
}

// ── Harness ──────────────────────────────────────────────────────────────────

struct Harness {
    pool: PgPool,
    schema: String,
    router: axum::Router,
}

impl Harness {
    async fn new() -> Option<Self> {
        let Ok(dsn) = std::env::var("BACKBONE_ORM_RLS_DSN") else {
            eprintln!("skipping: BACKBONE_ORM_RLS_DSN not set");
            return None;
        };
        let pool = PgPoolOptions::new()
            .max_connections(4)
            .connect(&dsn)
            .await
            .unwrap();
        let schema = format!("fp_{}", &Uuid::new_v4().simple().to_string()[..8]);
        sqlx::raw_sql(&format!(
            "CREATE SCHEMA {schema};
             CREATE TABLE {schema}.deals (
                 id        uuid PRIMARY KEY,
                 name      text NOT NULL,
                 bid_value double precision,
                 metadata  jsonb NOT NULL DEFAULT '{{}}'::jsonb
             );"
        ))
        .execute(&pool)
        .await
        .unwrap();
        let repo = Arc::new(DealRepo(backbone_orm::GenericCrudRepository::new(
            pool.clone(),
            &format!("{schema}.deals"),
        )));
        let service = DealService::new(repo, Arc::new(NoLifecycle), Arc::new(QuietPublisher));
        let router =
            BackboneCrudHandler::<DealService, Deal, CreateDeal, UpdateDeal, DealResponse>::routes(
                Arc::new(service),
                "/deals",
            );
        Some(Self {
            pool,
            schema,
            router,
        })
    }

    async fn teardown(self) {
        sqlx::raw_sql(&format!("DROP SCHEMA {} CASCADE", self.schema))
            .execute(&self.pool)
            .await
            .unwrap();
    }

    /// One request as `caller` (its grants), with no caller when `None`.
    async fn call(
        &self,
        method: &str,
        path: &str,
        caller: Option<&AuthContext>,
        body: Option<String>,
    ) -> (StatusCode, Value) {
        let mut builder = Request::builder()
            .method(method)
            .uri(path)
            .header("content-type", "application/json");
        if let Some(ctx) = caller {
            builder = builder.extension(ctx.clone());
        }
        let req = builder.body(Body::from(body.unwrap_or_default())).unwrap();
        let res = self.router.clone().oneshot(req).await.unwrap();
        let status = res.status();
        let bytes = axum::body::to_bytes(res.into_body(), 1 << 20)
            .await
            .unwrap();
        (
            status,
            serde_json::from_slice(&bytes).unwrap_or(Value::Null),
        )
    }

    fn holder() -> AuthContext {
        grants(&["deal:read_bid", "deal:create", "deal:update", "deal:read"])
    }
    fn non_holder() -> AuthContext {
        grants(&["deal:create", "deal:update", "deal:read"])
    }
}

struct NoLifecycle;

#[async_trait::async_trait]
impl backbone_core::service::ServiceLifecycle<Deal> for NoLifecycle {
    async fn before_create(&self, _e: &mut Deal) -> ServiceResult<()> {
        Ok(())
    }
    async fn after_create(&self, _e: &Deal) -> ServiceResult<()> {
        Ok(())
    }
}

fn grants(perms: &[&str]) -> AuthContext {
    AuthContext {
        user_id: "u".into(),
        roles: vec![],
        permissions: perms.iter().map(|p| p.to_string()).collect(),
    }
}

fn violation_codes(body: &Value) -> Vec<String> {
    body["violations"]
        .as_array()
        .map(|v| {
            v.iter()
                .filter_map(|x| x["code"].as_str().map(str::to_string))
                .collect()
        })
        .unwrap_or_default()
}

fn reason_of(body: &Value) -> Option<&str> {
    body["violations"][0]["params"]["reason"].as_str()
}

macro_rules! harness {
    () => {
        match Harness::new().await {
            Some(h) => h,
            None => return,
        }
    };
}

// ── The proofs ────────────────────────────────────────────────────────────────

/// The same row answers the holder the value, the non-holder `null` with the key present, and
/// `*:*` the value — on the record, in the list, and in the write's own response.
#[tokio::test]
async fn a_gated_field_is_redacted_to_null_for_a_non_holder_on_every_read() {
    let h = harness!();
    let holder = Harness::holder();
    let god = grants(&["*:*"]);
    let (status, body) = h
        .call(
            "POST",
            "/deals",
            Some(&holder),
            Some(r#"{"name": "Big", "bidValue": 125000.5}"#.into()),
        )
        .await;
    assert_eq!(status, StatusCode::CREATED, "{body}");
    assert_eq!(
        body["data"]["bidValue"],
        serde_json::json!(125000.5),
        "the holder's own write answers the value"
    );
    let id = body["data"]["id"].as_str().unwrap().to_string();

    let (_, denied) = h
        .call(
            "GET",
            &format!("/deals/{id}"),
            Some(&Harness::non_holder()),
            None,
        )
        .await;
    assert_eq!(
        denied["data"].get("bidValue"),
        Some(&serde_json::Value::Null),
        "redacted, not omitted: {denied}"
    );
    assert_eq!(denied["data"]["name"], "Big", "the other fields are whole");

    let (_, held) = h
        .call("GET", &format!("/deals/{id}"), Some(&holder), None)
        .await;
    assert_eq!(held["data"]["bidValue"], serde_json::json!(125000.5));
    let (_, seen) = h
        .call("GET", &format!("/deals/{id}"), Some(&god), None)
        .await;
    assert_eq!(
        seen["data"]["bidValue"],
        serde_json::json!(125000.5),
        "*:* sees everything"
    );

    let (_, listed) = h
        .call("GET", "/deals", Some(&Harness::non_holder()), None)
        .await;
    let rows = listed["data"].as_array().unwrap();
    assert!(
        rows.iter()
            .all(|r| r.get("bidValue") == Some(&serde_json::Value::Null)),
        "every row of the list: {listed}"
    );

    // A caller with no context at all holds nothing: fail closed.
    let (_, bare) = h.call("GET", &format!("/deals/{id}"), None, None).await;
    assert_eq!(bare["data"].get("bidValue"), Some(&serde_json::Value::Null));
    h.teardown().await;
}

/// A non-holder may not ask a question about the value: the filter, the sort and the aggregate
/// that name it are refused `400 invalid_query` with reason `field_permission`, on the parameter
/// that carried the name; a holder's are not.
#[tokio::test]
async fn a_non_holder_may_not_filter_sort_or_aggregate_over_the_field() {
    let h = harness!();
    let holder = Harness::holder();
    let (status, _) = h
        .call(
            "POST",
            "/deals",
            Some(&holder),
            Some(r#"{"name": "Ask", "bidValue": 10.0}"#.into()),
        )
        .await;
    assert_eq!(status, StatusCode::CREATED);

    for path in [
        "/deals?bid_value=10.0",
        "/deals?sort_by=bidValue",
        "/deals/aggregate?group_by=name&sum=bid_value",
        "/deals/count?min=bid_value",
    ] {
        let (status, body) = h
            .call("GET", path, Some(&Harness::non_holder()), None)
            .await;
        assert_eq!(status, StatusCode::BAD_REQUEST, "{path}: {body}");
        assert_eq!(reason_of(&body), Some("field_permission"), "{path}: {body}");
        assert_eq!(
            body["violations"][0]["params"]["permission"], "deal:read_bid",
            "{path}: {body}"
        );
        let codes = violation_codes(&body);
        assert!(
            codes.iter().all(|c| c == "invalid_query"),
            "{path}: {codes:?}"
        );

        let (status, body) = h.call("GET", path, Some(&holder), None).await;
        let refused_for_permission =
            status == StatusCode::BAD_REQUEST && reason_of(&body) == Some("field_permission");
        assert!(!refused_for_permission, "{path}: a holder may ask: {body}");
    }
    h.teardown().await;
}

/// A write that names the field is a holder's act: a non-holder is refused `422` naming the
/// permission in either casing, a holder's write goes through, and a non-holder's write that
/// does not name the field is untouched.
#[tokio::test]
async fn a_non_holders_write_that_sets_the_field_is_refused_naming_the_permission() {
    let h = harness!();
    let holder = Harness::holder();
    let (_, body) = h
        .call(
            "POST",
            "/deals",
            Some(&holder),
            Some(r#"{"name": "Set", "bidValue": 5.0}"#.into()),
        )
        .await;
    let id = body["data"]["id"].as_str().unwrap().to_string();

    let (status, body) = h
        .call(
            "PATCH",
            &format!("/deals/{id}"),
            Some(&Harness::non_holder()),
            Some(r#"{"bidValue": 999.0}"#.into()),
        )
        .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{body}");
    assert_eq!(
        body["violations"][0]["code"], "field_permission_required",
        "{body}"
    );
    assert_eq!(
        body["violations"][0]["params"]["permission"], "deal:read_bid",
        "{body}"
    );

    let (status, body) = h
        .call(
            "PUT",
            &format!("/deals/{id}"),
            Some(&Harness::non_holder()),
            Some(r#"{"name": "Set", "bid_value": 999.0}"#.into()),
        )
        .await;
    assert_eq!(
        status,
        StatusCode::UNPROCESSABLE_ENTITY,
        "the snake_case spelling is read too: {body}"
    );

    let (status, body) = h
        .call(
            "POST",
            "/deals",
            Some(&Harness::non_holder()),
            Some(r#"{"name": "No", "bidValue": 1.0}"#.into()),
        )
        .await;
    assert_eq!(
        status,
        StatusCode::UNPROCESSABLE_ENTITY,
        "a create naming it too: {body}"
    );

    let (status, body) = h
        .call(
            "PATCH",
            &format!("/deals/{id}"),
            Some(&Harness::non_holder()),
            Some(r#"{"name": "Renamed"}"#.into()),
        )
        .await;
    assert_eq!(
        status,
        StatusCode::OK,
        "a write that does not name the field goes through: {body}"
    );
    assert_eq!(
        body["data"].get("bidValue"),
        Some(&serde_json::Value::Null),
        "and its answer still redacts for this caller"
    );

    let (status, body) = h
        .call(
            "PATCH",
            &format!("/deals/{id}"),
            Some(&holder),
            Some(r#"{"bidValue": 7.5}"#.into()),
        )
        .await;
    assert_eq!(status, StatusCode::OK, "a holder sets it: {body}");
    assert_eq!(body["data"]["bidValue"], serde_json::json!(7.5));
    h.teardown().await;
}
