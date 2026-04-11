//! Redis permission cache integration tests
//!
//! These tests require a running Redis instance and are marked with #[ignore].
//! Run with: `cargo test -p backbone-authorization --features redis -- --ignored`

#![cfg(feature = "redis")]

use backbone_authorization::cache::PermissionCacheBackend;
use backbone_authorization::redis_cache::RedisPermissionCache;
use backbone_authorization::types::{Permission, UserAction, RoleAction};
use std::collections::HashSet;

const REDIS_URL: &str = "redis://localhost:6379";
const TEST_PREFIX: &str = "test:authz:perms";

fn test_permissions() -> HashSet<Permission> {
    let mut perms = HashSet::new();
    perms.insert(Permission::User(UserAction::Read));
    perms.insert(Permission::User(UserAction::Create));
    perms.insert(Permission::Role(RoleAction::List));
    perms
}

async fn setup_cache() -> RedisPermissionCache {
    let cache = RedisPermissionCache::with_prefix(REDIS_URL, TEST_PREFIX)
        .await
        .expect("Failed to connect to Redis");
    cache.clear().await.expect("Failed to clear cache");
    cache
}

#[tokio::test]
#[ignore]
async fn test_redis_get_set() {
    let cache = setup_cache().await;
    let perms = test_permissions();

    cache.set("user1", &perms, 300).await.unwrap();

    let result = cache.get("user1").await.unwrap();
    assert!(result.is_some());
    assert_eq!(result.unwrap(), perms);

    // Non-existent user
    let result = cache.get("nonexistent").await.unwrap();
    assert!(result.is_none());
}

#[tokio::test]
#[ignore]
async fn test_redis_ttl_expiry() {
    let cache = setup_cache().await;
    let perms = test_permissions();

    cache.set("user1", &perms, 1).await.unwrap();
    assert!(cache.get("user1").await.unwrap().is_some());

    tokio::time::sleep(std::time::Duration::from_secs(2)).await;
    assert!(cache.get("user1").await.unwrap().is_none());
}

#[tokio::test]
#[ignore]
async fn test_redis_delete() {
    let cache = setup_cache().await;
    let perms = test_permissions();

    cache.set("user1", &perms, 300).await.unwrap();
    assert!(cache.get("user1").await.unwrap().is_some());

    cache.delete("user1").await.unwrap();
    assert!(cache.get("user1").await.unwrap().is_none());

    // Deleting non-existent key should not error
    cache.delete("nonexistent").await.unwrap();
}

#[tokio::test]
#[ignore]
async fn test_redis_clear() {
    let cache = setup_cache().await;
    let perms = test_permissions();

    cache.set("user1", &perms, 300).await.unwrap();
    cache.set("user2", &perms, 300).await.unwrap();
    cache.set("user3", &perms, 300).await.unwrap();

    cache.clear().await.unwrap();

    assert!(cache.get("user1").await.unwrap().is_none());
    assert!(cache.get("user2").await.unwrap().is_none());
    assert!(cache.get("user3").await.unwrap().is_none());
}
