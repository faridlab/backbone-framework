//! Backbone Core Module Tests

use backbone_core::{
    BaseEntity, Entity, Repository, SearchableRepository, SoftDeletableRepository,
    PaginatedRepository, BulkRepository, CrudRepository,
    STANDARD_ENDPOINTS, STANDARD_ENDPOINT_COUNT, VERSION,
};
use serde::{Serialize, Deserialize};
use uuid::Uuid;
use chrono::{DateTime, Utc};
use std::collections::HashMap;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
struct TestEntity {
    #[serde(flatten)]
    base: BaseEntity,
    name: String,
    description: Option<String>,
    status: TestStatus,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
enum TestStatus {
    Active,
    Inactive,
    Pending,
}

impl TestEntity {
    fn new(name: &str, description: Option<String>, status: TestStatus) -> Self {
        Self {
            base: BaseEntity::new(),
            name: name.to_string(),
            description,
            status,
        }
    }

    fn from_base(base: BaseEntity, name: String, description: Option<String>, status: TestStatus) -> Self {
        Self {
            base,
            name,
            description,
            status,
        }
    }
}

impl Entity for TestEntity {
    fn id(&self) -> &Uuid {
        &self.base.id
    }

    fn created_at(&self) -> DateTime<Utc> {
        self.base.created_at
    }

    fn updated_at(&self) -> DateTime<Utc> {
        self.base.updated_at
    }

    fn deleted_at(&self) -> Option<DateTime<Utc>> {
        self.base.deleted_at
    }
}

// Mock repository for testing
struct MockRepository {
    entities: std::collections::HashMap<Uuid, TestEntity>,
}

impl MockRepository {
    fn new() -> Self {
        Self {
            entities: std::collections::HashMap::new(),
        }
    }

    fn add_entity(&mut self, entity: TestEntity) {
        self.entities.insert(entity.base.id, entity);
    }
}

#[async_trait::async_trait]
impl Repository<TestEntity> for MockRepository {
    async fn create(&self, entity: &TestEntity) -> anyhow::Result<TestEntity> {
        let mut new_entity = entity.clone();
        new_entity.base = BaseEntity::new();
        Ok(new_entity)
    }

    async fn find_by_id(&self, id: &Uuid) -> anyhow::Result<Option<TestEntity>> {
        Ok(self.entities.get(id).cloned())
    }

    async fn update(&self, entity: &TestEntity) -> anyhow::Result<TestEntity> {
        Ok(entity.clone())
    }

    async fn delete(&self, id: &Uuid) -> anyhow::Result<bool> {
        Ok(self.entities.contains_key(id))
    }

    async fn list(&self, _page: u32, _limit: u32) -> anyhow::Result<Vec<TestEntity>> {
        Ok(self.entities.values().cloned().collect())
    }
}

#[async_trait::async_trait]
impl SearchableRepository<TestEntity> for MockRepository {
    async fn search(&self, criteria: HashMap<String, String>, _page: u32, _limit: u32) -> anyhow::Result<Vec<TestEntity>> {
        let mut results = Vec::new();

        for entity in self.entities.values() {
            let mut matches = true;

            // Simple search by name
            if let Some(name_filter) = criteria.get("name") {
                if !entity.name.contains(name_filter) {
                    matches = false;
                }
            }

            // Simple search by status
            if let Some(status_filter) = criteria.get("status") {
                let status_str = match entity.status {
                    TestStatus::Active => "active",
                    TestStatus::Inactive => "inactive",
                    TestStatus::Pending => "pending",
                };
                if status_str != status_filter {
                    matches = false;
                }
            }

            if matches {
                results.push(entity.clone());
            }
        }

        Ok((results.clone(), results.len() as u64))
    }

    async fn find_by_field(&self, field: &str, value: &str) -> Result<Option<TestEntity>, backbone_core::RepositoryError> {
        for entity in self.entities.values() {
            if field == "name" && entity.name == value {
                return Ok(Some(entity.clone()));
            }
        }
        Ok(None)
    }

    async fn find_all_by_field(&self, field: &str, value: &str, _page: u32, _limit: u32) -> Result<(Vec<TestEntity>, u64), backbone_core::RepositoryError> {
        let results: Vec<_> = self.entities.values()
            .filter(|e| {
                if field == "name" {
                    e.name == value
                } else {
                    false
                }
            })
            .cloned()
            .collect();
        let total = results.len() as u64;
        Ok((results, total))
    }
}

#[async_trait::async_trait]
impl SoftDeletableRepository<TestEntity> for MockRepository {
    async fn soft_delete(&self, id: &Uuid) -> anyhow::Result<bool> {
        Ok(self.entities.contains_key(id))
    }

    async fn restore(&self, id: &Uuid) -> anyhow::Result<bool> {
        Ok(self.entities.contains_key(id))
    }

    async fn list_deleted(&self, _page: u32, _limit: u32) -> anyhow::Result<Vec<TestEntity>> {
        Ok(self.entities
            .values()
            .filter(|e| e.is_deleted())
            .cloned()
            .collect())
    }

    async fn permanent_delete_all(&self) -> anyhow::Result<u64> {
        Ok(self.entities.len() as u64)
    }
}

#[async_trait::async_trait]
impl PaginatedRepository<TestEntity> for MockRepository {
    async fn paginate(&self, page: u32, limit: u32) -> anyhow::Result<(Vec<TestEntity>, u64)> {
        let all_entities: Vec<TestEntity> = self.entities.values().cloned().collect();
        let total = all_entities.len() as u64;

        let start = ((page - 1) * limit) as usize;
        let end = (start + limit as usize).min(all_entities.len());

        let page_entities = if start < all_entities.len() {
            all_entities[start..end].to_vec()
        } else {
            Vec::new()
        };

        Ok((page_entities, total))
    }
}

#[async_trait::async_trait]
impl BulkRepository<TestEntity> for MockRepository {
    async fn bulk_create(&self, entities: Vec<TestEntity>) -> anyhow::Result<Vec<TestEntity>> {
        Ok(entities.into_iter().map(|mut e| {
            e.base = BaseEntity::new();
            e
        }).collect())
    }

    async fn bulk_update(&self, ids: &[Uuid], _updates: HashMap<String, String>) -> anyhow::Result<usize> {
        let count = ids.iter().filter(|id| self.entities.contains_key(id)).count();
        Ok(count)
    }

    async fn bulk_delete(&self, ids: &[Uuid]) -> anyhow::Result<u64> {
        let count = ids.iter().filter(|id| self.entities.contains_key(id)).count() as u64;
        Ok(count)
    }
}

// Since CrudRepository is a supertrait, MockRepository implements it
impl CrudRepository<TestEntity> for MockRepository {}

#[test]
fn test_base_entity_creation() {
    let entity = BaseEntity::new();

    // Test that all fields are properly initialized
    assert_ne!(entity.id, Uuid::nil());
    assert_eq!(entity.created_at, entity.updated_at);
    assert!(entity.deleted_at.is_none());
    assert!(!entity.is_deleted());
}

#[test]
fn test_base_entity_delete_and_restore() {
    let mut entity = BaseEntity::new();
    let original_updated_at = entity.updated_at;

    // Test deletion
    entity.delete();
    assert!(entity.deleted_at.is_some());
    assert!(entity.is_deleted());
    assert!(entity.updated_at > original_updated_at);

    // Test restore
    let updated_at_before_restore = entity.updated_at;
    entity.restore();
    assert!(entity.deleted_at.is_none());
    assert!(!entity.is_deleted());
    assert!(entity.updated_at > updated_at_before_restore);
}

#[test]
fn test_base_entity_touch() {
    let mut entity = BaseEntity::new();
    let original_updated_at = entity.updated_at;

    // Add a small delay to ensure timestamp difference
    std::thread::sleep(std::time::Duration::from_millis(1));

    entity.touch();
    assert!(entity.updated_at > original_updated_at);
}

#[test]
fn test_entity_trait_for_base_entity() {
    let entity = BaseEntity::new();

    // Test Entity trait methods
    assert_eq!(Entity::id(&entity), &entity.id);
    assert_eq!(Entity::created_at(&entity), entity.created_at);
    assert_eq!(Entity::updated_at(&entity), entity.updated_at);
    assert_eq!(Entity::deleted_at(&entity), None);
    assert!(!Entity::is_deleted(&entity));
}

#[test]
fn test_custom_entity_creation() {
    let entity = TestEntity::new(
        "Test Entity",
        Some("Test Description".to_string()),
        TestStatus::Active,
    );

    assert_eq!(entity.name, "Test Entity");
    assert_eq!(entity.description, Some("Test Description".to_string()));
    assert_eq!(entity.status, TestStatus::Active);
    assert!(!entity.is_deleted());
    assert_ne!(entity.id(), &Uuid::nil());
}

#[test]
fn test_custom_entity_serialization() {
    let entity = TestEntity::new(
        "Test Entity",
        None,
        TestStatus::Pending,
    );

    // Test JSON serialization
    let json = serde_json::to_string(&entity).unwrap();
    assert!(json.contains("Test Entity"));

    // Test JSON deserialization
    let deserialized: TestEntity = serde_json::from_str(&json).unwrap();
    assert_eq!(deserialized.name, entity.name);
    assert_eq!(deserialized.status, entity.status);
    assert_eq!(deserialized.id(), entity.id());
}

#[test]
fn test_custom_entity_delete_and_restore() {
    let mut entity = TestEntity::new(
        "Test Entity",
        None,
        TestStatus::Active,
    );

    // Test deletion
    entity.base.delete();
    assert!(entity.is_deleted());

    // Test restore
    entity.base.restore();
    assert!(!entity.is_deleted());
}

#[tokio::test]
async fn test_repository_basic_operations() -> anyhow::Result<()> {
    let repository = MockRepository::new();
    let entity = TestEntity::new("Test", None, TestStatus::Active);

    // Test create
    let created = repository.create(&entity).await?;
    assert_ne!(created.id(), entity.id()); // Should have new ID
    assert_eq!(created.name, entity.name);

    // Test find_by_id (entity not in mock repository)
    let found = repository.find_by_id(created.id()).await?;
    assert!(found.is_none());

    // Test delete (entity not in repository)
    let deleted = repository.delete(created.id()).await?;
    assert!(!deleted);

    // Test list (empty repository)
    let list = repository.list(1, 10).await?;
    assert!(list.is_empty());

    Ok(())
}

#[tokio::test]
async fn test_repository_with_populated_data() -> anyhow::Result<()> {
    let mut repository = MockRepository::new();
    let entity1 = TestEntity::new("Entity 1", None, TestStatus::Active);
    let entity2 = TestEntity::new("Entity 2", Some("Description".to_string()), TestStatus::Inactive);

    // Add entities to mock repository
    repository.add_entity(entity1.clone());
    repository.add_entity(entity2.clone());

    // Test find_by_id
    let found = repository.find_by_id(entity1.id()).await?;
    assert!(found.is_some());
    assert_eq!(found.unwrap().name, "Entity 1");

    // Test list
    let list = repository.list(1, 10).await?;
    assert_eq!(list.len(), 2);

    // Test update
    let updated = repository.update(&entity1).await?;
    assert_eq!(updated.id(), entity1.id());

    // Test delete
    let deleted = repository.delete(entity1.id()).await?;
    assert!(deleted);

    Ok(())
}

#[tokio::test]
async fn test_searchable_repository() -> anyhow::Result<()> {
    let mut repository = MockRepository::new();

    // Add test entities
    repository.add_entity(TestEntity::new("Active User 1", None, TestStatus::Active));
    repository.add_entity(TestEntity::new("Active User 2", Some("Desc".to_string()), TestStatus::Active));
    repository.add_entity(TestEntity::new("Inactive User", None, TestStatus::Inactive));

    // Test search by name
    let mut criteria = HashMap::new();
    criteria.insert("name".to_string(), "Active".to_string());
    let results = repository.search(criteria, 1, 10).await?;
    assert_eq!(results.len(), 2);

    // Test search by status
    let mut criteria = HashMap::new();
    criteria.insert("status".to_string(), "active".to_string());
    let results = repository.search(criteria, 1, 10).await?;
    assert_eq!(results.len(), 2);

    // Test search by non-matching criteria
    let mut criteria = HashMap::new();
    criteria.insert("name".to_string(), "NonExistent".to_string());
    let results = repository.search(criteria, 1, 10).await?;
    assert!(results.is_empty());

    // Test count
    let mut criteria = HashMap::new();
    criteria.insert("status".to_string(), "inactive".to_string());
    let count = repository.count(criteria).await?;
    assert_eq!(count, 1);

    Ok(())
}

#[tokio::test]
async fn test_soft_deletable_repository() -> anyhow::Result<()> {
    let mut repository = MockRepository::new();

    let active_entity = TestEntity::new("Active Entity", None, TestStatus::Active);
    let mut deleted_entity = TestEntity::new("Deleted Entity", None, TestStatus::Active);
    deleted_entity.base.delete();

    repository.add_entity(active_entity.clone());
    repository.add_entity(deleted_entity.clone());

    // Test soft delete
    let deleted = repository.soft_delete(active_entity.id()).await?;
    assert!(deleted);

    // Test restore
    let restored = repository.restore(active_entity.id()).await?;
    assert!(restored);

    // Test list deleted
    let deleted_list = repository.list_deleted(1, 10).await?;
    assert_eq!(deleted_list.len(), 1);
    assert_eq!(deleted_list[0].name, "Deleted Entity");

    // Test permanent delete all
    let deleted_count = repository.permanent_delete_all().await?;
    assert_eq!(deleted_count, 2);

    Ok(())
}

#[tokio::test]
async fn test_paginated_repository() -> anyhow::Result<()> {
    let mut repository = MockRepository::new();

    // Add multiple entities
    for i in 1..=10 {
        repository.add_entity(TestEntity::new(&format!("Entity {}", i), None, TestStatus::Active));
    }

    // Test pagination
    let (page1, total) = repository.paginate(1, 3).await?;
    assert_eq!(page1.len(), 3);
    assert_eq!(total, 10);

    let (page2, total) = repository.paginate(2, 3).await?;
    assert_eq!(page2.len(), 3);
    assert_eq!(total, 10);

    let (page4, total) = repository.paginate(4, 3).await?;
    assert_eq!(page4.len(), 1); // Last page with 1 item
    assert_eq!(total, 10);

    let (page5, total) = repository.paginate(5, 3).await?;
    assert!(page5.is_empty()); // Empty page beyond range
    assert_eq!(total, 10);

    Ok(())
}

#[tokio::test]
async fn test_bulk_repository() -> anyhow::Result<()> {
    let mut repository = MockRepository::new();

    // Add some entities for bulk operations
    let entity1 = TestEntity::new("Entity 1", None, TestStatus::Active);
    let entity2 = TestEntity::new("Entity 2", None, TestStatus::Active);
    let entity3 = TestEntity::new("Entity 3", None, TestStatus::Active);

    repository.add_entity(entity1.clone());
    repository.add_entity(entity2.clone());
    repository.add_entity(entity3.clone());

    // Test bulk create
    let new_entities = vec![
        TestEntity::new("New 1", None, TestStatus::Pending),
        TestEntity::new("New 2", None, TestStatus::Pending),
    ];
    let created = repository.bulk_create(new_entities).await?;
    assert_eq!(created.len(), 2);

    // Test bulk update
    let ids = vec![*entity1.id(), *entity2.id()];
    let mut updates = HashMap::new();
    updates.insert("name".to_string(), "Updated".to_string());
    let updated_count = repository.bulk_update(&ids, updates).await?;
    assert_eq!(updated_count, 2);

    // Test bulk delete
    let delete_ids = vec![*entity1.id(), *entity3.id()];
    let deleted_count = repository.bulk_delete(&delete_ids).await?;
    assert_eq!(deleted_count, 2);

    Ok(())
}

#[test]
fn test_constants_and_version() {
    // Test standard endpoints constant
    assert_eq!(STANDARD_ENDPOINT_COUNT, 11);
    assert_eq!(STANDARD_ENDPOINTS.len(), 11);

    // Verify specific endpoints
    assert!(STANDARD_ENDPOINTS.contains(&"GET /api/v1/{collection}"));
    assert!(STANDARD_ENDPOINTS.contains(&"POST /api/v1/{collection}"));
    assert!(STANDARD_ENDPOINTS.contains(&"DELETE /api/v1/{collection}/empty"));

    // Test version constant
    assert!(!VERSION.is_empty());
    assert_eq!(VERSION, env!("CARGO_PKG_VERSION"));
}

#[test]
fn test_entity_serialization_roundtrip() {
    // Test full serialization roundtrip for custom entity
    let original = TestEntity::new(
        "Test Entity",
        Some("Test Description".to_string()),
        TestStatus::Active,
    );

    // Serialize to JSON
    let json = serde_json::to_string_pretty(&original).unwrap();

    // Deserialize from JSON
    let deserialized: TestEntity = serde_json::from_str(&json).unwrap();

    // Verify all fields match
    assert_eq!(deserialized.name, original.name);
    assert_eq!(deserialized.description, original.description);
    assert_eq!(deserialized.status, original.status);
    assert_eq!(deserialized.id(), original.id());
    assert_eq!(deserialized.created_at(), original.created_at());
    assert_eq!(deserialized.updated_at(), original.updated_at());
    assert_eq!(deserialized.deleted_at(), original.deleted_at());
}

#[test]
fn test_multiple_entity_types() {
    // Test that we can have different entity types using the same base
    #[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
    struct AnotherEntity {
        #[serde(flatten)]
        base: BaseEntity,
        title: String,
        value: i32,
    }

    impl Entity for AnotherEntity {
        fn id(&self) -> &Uuid {
            &self.base.id
        }

        fn created_at(&self) -> DateTime<Utc> {
            self.base.created_at
        }

        fn updated_at(&self) -> DateTime<Utc> {
            self.base.updated_at
        }

        fn deleted_at(&self) -> Option<DateTime<Utc>> {
            self.base.deleted_at
        }
    }

    let entity1 = TestEntity::new("Test", None, TestStatus::Active);
    let entity2 = AnotherEntity {
        base: BaseEntity::new(),
        title: "Another Test".to_string(),
        value: 42,
    };

    // Verify they have different IDs
    assert_ne!(entity1.id(), entity2.id());

    // Verify they can both be serialized
    let json1 = serde_json::to_string(&entity1).unwrap();
    let json2 = serde_json::to_string(&entity2).unwrap();

    assert!(json1.contains("Test"));
    assert!(json2.contains("Another Test"));
    assert!(json2.contains("42"));
}

#[test]
fn test_entity_lifecycle() {
    let mut entity = TestEntity::new("Lifecycle Test", None, TestStatus::Pending);

    // Track initial timestamps
    let created_at = entity.created_at();
    let initial_updated_at = entity.updated_at();

    // Test initial state
    assert!(!entity.is_deleted());
    assert_eq!(entity.created_at(), created_at);
    assert_eq!(entity.updated_at(), initial_updated_at);

    // Add small delay to ensure timestamp differences
    std::thread::sleep(std::time::Duration::from_millis(1));

    // Test touch
    entity.base.touch();
    assert!(entity.updated_at() > initial_updated_at);

    // Test delete
    let touched_updated_at = entity.updated_at();
    std::thread::sleep(std::time::Duration::from_millis(1));

    entity.base.delete();
    assert!(entity.is_deleted());
    assert!(entity.deleted_at().is_some());
    assert!(entity.updated_at() > touched_updated_at);

    // Test restore
    let deleted_updated_at = entity.updated_at();
    std::thread::sleep(std::time::Duration::from_millis(1));

    entity.base.restore();
    assert!(!entity.is_deleted());
    assert!(entity.deleted_at().is_none());
    assert!(entity.updated_at() > deleted_updated_at);

    // Verify created_at never changes
    assert_eq!(entity.created_at(), created_at);
}