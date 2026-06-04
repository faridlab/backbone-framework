//! Compile-test for `impl_crud_repository!`.
//!
//! The macro expands `backbone_core::` / `backbone_orm::` paths and is only
//! exercised in consumer crates (where both are dependencies). This integration
//! test reproduces that setup so a signature mismatch in the macro — which would
//! otherwise break every generated repository at consumer build time — is caught
//! here instead. The assertion is simply that this file compiles; the async
//! helper below pins the expected batch-method signatures for both variants.

use std::collections::HashMap;

use backbone_core::{CrudRepository, PersistentEntity, RepositoryError};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

// ── Soft-delete entity + repository ──────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
struct Thing {
    id: String,
    name: String,
}

impl PersistentEntity for Thing {
    fn entity_id(&self) -> String {
        self.id.clone()
    }
    fn set_entity_id(&mut self, id: String) {
        self.id = id;
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

impl backbone_orm::EntityRepoMeta for Thing {
    fn column_types() -> HashMap<String, String> {
        HashMap::new()
    }
    fn search_fields() -> &'static [&'static str] {
        &[]
    }
}

struct ThingRepo(backbone_orm::GenericCrudRepository<Thing, backbone_orm::SoftDelete>);

impl std::ops::Deref for ThingRepo {
    type Target = backbone_orm::GenericCrudRepository<Thing, backbone_orm::SoftDelete>;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

backbone_core::impl_crud_repository!(ThingRepo, Thing, soft_delete);

// ── Hard-delete entity + repository ──────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
struct LogLine {
    id: String,
}

impl PersistentEntity for LogLine {
    fn entity_id(&self) -> String {
        self.id.clone()
    }
    fn set_entity_id(&mut self, id: String) {
        self.id = id;
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

impl backbone_orm::EntityRepoMeta for LogLine {
    fn column_types() -> HashMap<String, String> {
        HashMap::new()
    }
    fn search_fields() -> &'static [&'static str] {
        &[]
    }
}

struct LogRepo(backbone_orm::GenericCrudRepository<LogLine, backbone_orm::HardDelete>);

impl std::ops::Deref for LogRepo {
    type Target = backbone_orm::GenericCrudRepository<LogLine, backbone_orm::HardDelete>;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

backbone_core::impl_crud_repository!(LogRepo, LogLine, no_soft_delete);

// Pins the batch-method signatures the macro must produce for both variants.
// Never called — compiling is the assertion.
#[allow(dead_code)]
async fn _batch_signatures(thing: &ThingRepo, log: &LogRepo) {
    let ids = vec!["a".to_string()];
    let _: Result<u64, RepositoryError> = thing.bulk_soft_delete(&ids).await;
    let _: Result<Vec<Thing>, RepositoryError> = thing.bulk_restore(&ids).await;
    let _: Result<u64, RepositoryError> = thing.bulk_hard_delete(&ids).await;
    let _: Result<Vec<Thing>, RepositoryError> = thing.restore_all().await;
    let _: Result<Vec<Thing>, RepositoryError> = thing.bulk_update(Vec::new()).await;

    let _: Result<u64, RepositoryError> = log.bulk_soft_delete(&ids).await;
    let _: Result<Vec<LogLine>, RepositoryError> = log.bulk_restore(&ids).await;
    let _: Result<Vec<LogLine>, RepositoryError> = log.restore_all().await;
}

#[test]
fn macro_expands_batch_methods_for_both_variants() {
    // If the macro produced mismatched signatures this test file would fail to
    // compile. Reaching runtime means both variants expanded correctly.
}
