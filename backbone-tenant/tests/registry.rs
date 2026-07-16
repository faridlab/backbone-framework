//! Behavioural tests for the tenant-runtime registry.
//!
//! No database: a stub factory stands in for "open a pool + build the modules" and counts how many
//! times each tenant is built. That count is exactly what the registry's guarantees are about, so it
//! is what the tests assert on.

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Duration;

use backbone_tenant::{RegistryError, TenantId, TenantRegistry, TenantRuntimeFactory};

/// A built "runtime" — just its tenant id and a unique build serial, enough to tell two builds apart.
#[derive(Debug)]
struct FakeRuntime {
    tenant: String,
    serial: usize,
}

#[derive(Debug, thiserror::Error)]
#[error("stub build failed for '{0}'")]
struct StubError(String);

/// Counts builds per call and can be told to fail (once) or to block (to force a race).
struct StubFactory {
    builds: AtomicUsize,
    fail_until: AtomicUsize,     // fail the first N builds, then succeed
    build_delay: Duration,       // held across the await to widen the race window
}

impl StubFactory {
    fn new() -> Self {
        Self {
            builds: AtomicUsize::new(0),
            fail_until: AtomicUsize::new(0),
            build_delay: Duration::ZERO,
        }
    }
    fn with_delay(delay: Duration) -> Self {
        Self { build_delay: delay, ..Self::new() }
    }
    fn fail_first(n: usize) -> Self {
        let f = Self::new();
        f.fail_until.store(n, Ordering::SeqCst);
        f
    }
    fn build_count(&self) -> usize {
        self.builds.load(Ordering::SeqCst)
    }
}

#[async_trait::async_trait]
impl TenantRuntimeFactory for StubFactory {
    type Runtime = FakeRuntime;
    type Error = StubError;

    async fn build(&self, tenant: &TenantId) -> Result<FakeRuntime, StubError> {
        let serial = self.builds.fetch_add(1, Ordering::SeqCst);
        if !self.build_delay.is_zero() {
            tokio::time::sleep(self.build_delay).await;
        }
        // Fail the first `fail_until` builds so the retry path can be exercised.
        if self.fail_until.load(Ordering::SeqCst) > 0 {
            self.fail_until.fetch_sub(1, Ordering::SeqCst);
            return Err(StubError(tenant.0.clone()));
        }
        Ok(FakeRuntime { tenant: tenant.0.clone(), serial })
    }
}

#[tokio::test]
async fn builds_once_then_serves_from_cache() {
    let reg = TenantRegistry::new(StubFactory::new(), 8);
    let t = TenantId::from("acme");

    let a = reg.get_or_build(&t).await.unwrap();
    let b = reg.get_or_build(&t).await.unwrap();

    assert_eq!(a.tenant, "acme");
    assert_eq!(a.serial, b.serial, "second call must return the cached runtime, not a rebuild");
    // The factory ran exactly once despite two resolves.
    assert_eq!(reg.len().await, 1);
}

#[tokio::test]
async fn distinct_tenants_get_distinct_runtimes() {
    let reg = TenantRegistry::new(StubFactory::new(), 8);
    let a = reg.get_or_build(&"a".into()).await.unwrap();
    let b = reg.get_or_build(&"b".into()).await.unwrap();
    assert_ne!(a.serial, b.serial);
    assert_eq!(reg.len().await, 2);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn concurrent_first_requests_build_once() {
    // The guarantee that matters most: a cold tenant hit by 50 simultaneous requests builds ONE
    // runtime (one pool, one module graph), not 50.
    let reg = Arc::new(TenantRegistry::new(
        StubFactory::with_delay(Duration::from_millis(25)),
        8,
    ));
    let t = TenantId::from("busy");

    let mut handles = Vec::new();
    for _ in 0..50 {
        let reg = Arc::clone(&reg);
        let t = t.clone();
        handles.push(tokio::spawn(async move { reg.get_or_build(&t).await.unwrap().serial }));
    }
    let serials: Vec<usize> = futures_join(handles).await;

    assert!(serials.iter().all(|&s| s == serials[0]), "all callers must share one build");
    assert_eq!(reg.len().await, 1);
}

#[tokio::test]
async fn a_failed_build_does_not_poison_the_slot() {
    // First build fails; the slot must stay empty so the next request retries and succeeds.
    let reg = TenantRegistry::new(StubFactory::fail_first(1), 8);
    let t = TenantId::from("flaky");

    let first = reg.get_or_build(&t).await;
    assert!(matches!(first, Err(RegistryError::Build { .. })), "first build should fail");

    let second = reg.get_or_build(&t).await;
    assert!(second.is_ok(), "a later request must retry, not inherit the failure");
    assert_eq!(second.unwrap().tenant, "flaky");
}

#[tokio::test]
async fn capacity_evicts_the_least_recently_used() {
    let reg = TenantRegistry::new(StubFactory::new(), 2);

    let _a = reg.get_or_build(&"a".into()).await.unwrap();
    let _b = reg.get_or_build(&"b".into()).await.unwrap();
    // Touch `a` so `b` becomes the LRU.
    let _a2 = reg.get_or_build(&"a".into()).await.unwrap();
    // Inserting `c` exceeds cap=2 → the LRU (`b`) is evicted, not `a`.
    let _c = reg.get_or_build(&"c".into()).await.unwrap();

    assert_eq!(reg.len().await, 2);
    assert!(reg.evict(&"a".into()).await, "a should still be resident");
    assert!(reg.evict(&"c".into()).await, "c should still be resident");
    // b is gone.
    assert!(!reg.evict(&"b".into()).await, "b should have been evicted as LRU");
}

#[tokio::test]
async fn eviction_does_not_kill_an_in_flight_holder() {
    let reg = TenantRegistry::new(StubFactory::new(), 8);
    let t = TenantId::from("acme");

    let held = reg.get_or_build(&t).await.unwrap(); // a request still holding the runtime
    assert!(reg.evict(&t).await);
    assert_eq!(reg.len().await, 0, "registry dropped its handle");

    // The in-flight holder is unaffected — the Arc kept it alive.
    assert_eq!(held.tenant, "acme");

    // A subsequent request rebuilds (fresh serial), proving the old one was truly removed.
    let fresh = reg.get_or_build(&t).await.unwrap();
    assert_ne!(fresh.serial, held.serial);
}

#[tokio::test]
async fn evict_idle_drops_only_stale_tenants() {
    let reg = TenantRegistry::new(StubFactory::new(), 8);
    reg.get_or_build(&"old".into()).await.unwrap(); // tick 1
    reg.get_or_build(&"mid".into()).await.unwrap(); // tick 2
    reg.get_or_build(&"new".into()).await.unwrap(); // tick 3

    // Keep only tenants within 1 tick of the newest (tick 3): keeps `mid` (2) and `new` (3), drops
    // `old` (1). cutoff = 3 - 1 = 2, retain last > 2.
    let evicted = reg.evict_idle(1).await;
    assert_eq!(evicted, 1);
    assert!(!reg.evict(&"old".into()).await, "old was idle and should be gone");
    assert!(reg.evict(&"mid".into()).await);
    assert!(reg.evict(&"new".into()).await);
}

#[tokio::test]
#[should_panic(expected = "max_tenants must be > 0")]
async fn zero_capacity_is_rejected() {
    let _ = TenantRegistry::new(StubFactory::new(), 0);
}

/// Minimal join-all so the test file needs no `futures` dependency.
async fn futures_join<T: Send + 'static>(handles: Vec<tokio::task::JoinHandle<T>>) -> Vec<T> {
    let mut out = Vec::with_capacity(handles.len());
    for h in handles {
        out.push(h.await.unwrap());
    }
    out
}
