//! Per-tenant runtime registry (ADR-0006, routing option **B1**).
//!
//! ADR-0005 makes the tenant the **database**. A request must therefore reach its tenant's
//! resources — in production, a `PgPool` plus the module graph built over it. But modules bind their
//! pool at **build time** (`Module::builder().with_database(pool).build()`), so the way to have
//! per-tenant databases without rewriting every module is to build the whole runtime **once per
//! tenant** and cache it, keyed by tenant id.
//!
//! This crate is that cache, and nothing more. It is deliberately generic over what a "runtime" is
//! (`F::Runtime`) and how one is built (`TenantRuntimeFactory`), so the routing logic — build-once,
//! cache, evict — is testable without a database. The composition root supplies the real factory
//! (open a pool to `tenant_<id>`, run the module builders); this crate never imports `sqlx` or a
//! module.
//!
//! ## Scope boundary — what does NOT belong here
//!
//! This is **routing plumbing that sits ABOVE modules** (it dispatches a request into a tenant's
//! module graph), so it is a framework crate, not a `module`. The tenant **domain** — the `Tenant`
//! entity, membership, plans, provisioning lifecycle, billing — is a *separate* schema-driven module
//! (the "control plane" of [ADR-0006](../../docs/handbook/adr/0006-per-tenant-database-isolation.md)),
//! under its own name. Do not grow entities, persistence, or business rules in here: a crate must
//! stay plumbing (`backbone-framework/CLAUDE.md`), and the router cannot depend on the domain it
//! routes to without inverting the layering.
//!
//! ## Guarantees
//!
//! - **Build-once under concurrency.** Two simultaneous first requests for the same tenant build the
//!   runtime exactly once; the second awaits the first's result. (A cold build is expensive — a pool
//!   plus N module graphs — so a double build is not just wasteful, it can double a tenant's
//!   connections.)
//! - **Failures don't poison, and are not kept.** If a build fails, its entry is removed and the next
//!   request retries. A tenant whose database was briefly unreachable recovers on its own, and a
//!   valid-looking slug that names no provisioned tenant never holds a slot.
//! - **Eviction never kills in-flight work.** Evicting a tenant drops the registry's handle; any
//!   request already holding the runtime keeps it alive until it finishes (it is an `Arc`).
//! - **Bounded.** `max_tenants` caps built runtimes; the tenant least recently resolved
//!   *successfully* is evicted when a new build would exceed the cap. Builds in flight and builds
//!   that fail never count against the cap, so requests for unknown tenants cannot push
//!   established tenants out. Idle eviction (by wall clock) layers on top via
//!   [`TenantRegistry::evict_idle`], driven by the caller so this crate needs no clock.

use std::collections::HashMap;
use std::sync::Arc;

use tokio::sync::{Mutex, OnceCell};

pub mod resolve;

#[cfg(feature = "axum")]
pub mod axum_router;

#[cfg(feature = "provision")]
pub mod provision;

/// A tenant's stable identifier — a subdomain (`acme`) or an opaque id, whatever the router resolves.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct TenantId(pub String);

impl TenantId {
    /// Borrow the id as a string slice.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl<S: Into<String>> From<S> for TenantId {
    fn from(s: S) -> Self {
        TenantId(s.into())
    }
}

/// Builds a tenant's runtime on first use. The composition root implements this: open a pool to the
/// tenant's database and construct the module graph over it.
#[async_trait::async_trait]
pub trait TenantRuntimeFactory: Send + Sync {
    /// What a built tenant holds — e.g. `(PgPool, Router)`.
    type Runtime: Send + Sync;
    /// Why a build failed — unknown tenant, database unreachable, migrations pending, …
    type Error: std::error::Error + Send + Sync + 'static;

    /// Build the runtime for `tenant`. Called at most once per resident tenant (retried only after a
    /// failure or an eviction).
    async fn build(&self, tenant: &TenantId) -> Result<Self::Runtime, Self::Error>;
}

/// Registry failures surfaced to a caller.
#[derive(Debug, thiserror::Error)]
pub enum RegistryError<E> {
    /// The factory could not build the tenant's runtime.
    #[error("failed to build runtime for tenant '{tenant}': {source}")]
    Build {
        /// The tenant whose build failed.
        tenant: String,
        /// The factory's error.
        #[source]
        source: E,
    },
}

struct Entry<R> {
    /// `OnceCell` gives build-once-under-concurrency for free: the first caller runs the factory, the
    /// rest await it, and a failure leaves the cell empty for a later retry.
    cell: Arc<OnceCell<Arc<R>>>,
    /// Access tick for LRU ordering. Higher = more recently used.
    last: u64,
}

struct Inner<R> {
    entries: HashMap<TenantId, Entry<R>>,
    /// Monotonic access counter — LRU order without a wall clock, so eviction is deterministic.
    tick: u64,
}

/// A bounded, lazily-populated cache of per-tenant runtimes.
///
/// Construct with a factory and a capacity; call [`get_or_build`](Self::get_or_build) on every
/// request after the tenant is resolved.
pub struct TenantRegistry<F: TenantRuntimeFactory> {
    factory: F,
    max_tenants: usize,
    inner: Mutex<Inner<F::Runtime>>,
}

impl<F: TenantRuntimeFactory> TenantRegistry<F> {
    /// Create a registry over `factory`, holding at most `max_tenants` resident runtimes.
    ///
    /// # Panics
    /// Panics if `max_tenants == 0` — a registry that can hold nothing serves nothing.
    pub fn new(factory: F, max_tenants: usize) -> Self {
        assert!(max_tenants > 0, "max_tenants must be > 0");
        Self {
            factory,
            max_tenants,
            inner: Mutex::new(Inner {
                entries: HashMap::new(),
                tick: 0,
            }),
        }
    }

    /// Resolve `tenant` to its runtime, building and caching it on first use.
    ///
    /// The returned `Arc` keeps the runtime alive for the duration of the request even if the tenant
    /// is evicted meanwhile. Concurrent first-callers for the same tenant share one build.
    pub async fn get_or_build(
        &self,
        tenant: &TenantId,
    ) -> Result<Arc<F::Runtime>, RegistryError<F::Error>> {
        // Phase 1 (locked, brief): find or create the tenant's cell. The lock is released before
        // the (slow) build so other tenants are never blocked by it. A new entry is not yet
        // resident: it neither counts against capacity nor takes part in LRU order until its
        // build succeeds.
        let cell = {
            let mut inner = self.inner.lock().await;
            let tick = inner.tick;
            let entry = inner.entries.entry(tenant.clone()).or_insert_with(|| Entry {
                cell: Arc::new(OnceCell::new()),
                last: tick,
            });
            Arc::clone(&entry.cell)
        };

        // Phase 2 (unlocked): build-once. `get_or_try_init` runs the factory for the first caller and
        // makes the rest await it; on error the cell stays empty and a later call retries.
        let built = cell
            .get_or_try_init(|| async {
                self.factory.build(tenant).await.map(Arc::new)
            })
            .await;
        let runtime = match built {
            Ok(runtime) => Arc::clone(runtime),
            Err(source) => {
                // Never keep a failed build: a slug that names no provisioned tenant must not
                // hold a slot, or enough of them push real tenants out at the next eviction.
                self.forget_failed(tenant, &cell).await;
                return Err(RegistryError::Build {
                    tenant: tenant.0.clone(),
                    source,
                });
            }
        };

        // Phase 3 (locked, brief): mark this tenant most-recently-used by a SUCCESSFUL resolve, then
        // enforce the capacity bound — evicting the least-recently-used built tenant other than this
        // one.
        self.touch_and_evict(tenant, &cell).await;

        Ok(runtime)
    }

    /// Drop `tenant`'s entry after a failed build, unless another request has meanwhile replaced
    /// it or built it — a concurrent retry that succeeded keeps its slot.
    async fn forget_failed(&self, tenant: &TenantId, cell: &Arc<OnceCell<Arc<F::Runtime>>>) {
        let mut inner = self.inner.lock().await;
        let ours = inner
            .entries
            .get(tenant)
            .is_some_and(|e| Arc::ptr_eq(&e.cell, cell) && !e.cell.initialized());
        if ours {
            inner.entries.remove(tenant);
        }
    }

    /// Record a successful resolve of `tenant` and trim the registry back to capacity.
    async fn touch_and_evict(&self, tenant: &TenantId, cell: &Arc<OnceCell<Arc<F::Runtime>>>) {
        let mut inner = self.inner.lock().await;
        inner.tick += 1;
        let tick = inner.tick;
        // A failed concurrent caller may have removed the entry between our build and this
        // lock; the build succeeded, so put it back rather than lose it.
        let entry = inner.entries.entry(tenant.clone()).or_insert_with(|| Entry {
            cell: Arc::clone(cell),
            last: tick,
        });
        entry.last = tick;

        // Only built tenants count against the bound and only they are eviction victims; an
        // entry mid-build is transient (it is either built or removed when its build ends).
        loop {
            let resident = inner.entries.values().filter(|e| e.cell.initialized()).count();
            if resident <= self.max_tenants {
                break;
            }
            let victim = inner
                .entries
                .iter()
                .filter(|(id, e)| *id != tenant && e.cell.initialized())
                .min_by_key(|(_, e)| e.last)
                .map(|(id, _)| id.clone());
            match victim {
                Some(id) => {
                    inner.entries.remove(&id);
                }
                // Only `tenant` remains built and we are still over cap — impossible unless
                // max_tenants < 1, which the constructor forbids. Stop rather than loop forever.
                None => break,
            }
        }
    }

    /// Number of resident tenants (built or mid-build).
    pub async fn len(&self) -> usize {
        self.inner.lock().await.entries.len()
    }

    /// Whether the registry currently holds no tenants.
    pub async fn is_empty(&self) -> bool {
        self.len().await == 0
    }

    /// Drop a single tenant's runtime. In-flight requests holding it are unaffected.
    pub async fn evict(&self, tenant: &TenantId) -> bool {
        self.inner.lock().await.entries.remove(tenant).is_some()
    }

    /// Drop every tenant's runtime (e.g. on a global config reload).
    pub async fn evict_all(&self) {
        self.inner.lock().await.entries.clear();
    }

    /// Evict tenants not accessed within the last `keep` accesses.
    ///
    /// This crate keeps no clock; the caller expresses idleness in access-ticks (or schedules real
    /// wall-clock eviction by calling [`evict`](Self::evict) itself). `keep` is a window over the LRU
    /// tick: any tenant whose last access is more than `keep` ticks behind the newest is dropped.
    /// Returns how many were evicted.
    pub async fn evict_idle(&self, keep: u64) -> usize {
        let mut inner = self.inner.lock().await;
        let newest = inner.tick;
        // Drop tenants MORE than `keep` ticks behind the newest access; keep those within the window.
        // `keep = 0` retains only tenants touched at the newest tick.
        let cutoff = newest.saturating_sub(keep);
        let before = inner.entries.len();
        inner.entries.retain(|_, e| e.last >= cutoff);
        before - inner.entries.len()
    }
}
