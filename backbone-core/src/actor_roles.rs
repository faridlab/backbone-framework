//! The roles of whoever is writing, for rules that ask (`$actor.has_role('admin')`).
//!
//! Roles are not on the request: the composing service knows where they live (serpa reads
//! its authorization tables) and installs an [`ActorRoles`] once at startup. A compiled rule
//! asks it only when the rule runs. Without one installed, a role rule reads null and holds,
//! so a deployment that never wires roles is reported once and refused nothing.

use std::sync::{Arc, OnceLock, RwLock};

use async_trait::async_trait;

/// Where the writer's roles come from.
#[async_trait]
pub trait ActorRoles: Send + Sync {
    /// Whether `actor` holds `role`; `None` when it cannot be told (an unknown actor, a
    /// failed lookup). Role names compare case-insensitively.
    async fn holds(&self, actor: &str, role: &str) -> Option<bool>;
}

fn slot() -> &'static RwLock<Option<Arc<dyn ActorRoles>>> {
    static SLOT: OnceLock<RwLock<Option<Arc<dyn ActorRoles>>>> = OnceLock::new();
    SLOT.get_or_init(|| RwLock::new(None))
}

/// Install the roles source the role rules ask. A later install replaces it.
pub fn install(roles: Arc<dyn ActorRoles>) {
    *slot().write().unwrap_or_else(|e| e.into_inner()) = Some(roles);
}

pub(crate) fn installed() -> Option<Arc<dyn ActorRoles>> {
    slot().read().unwrap_or_else(|e| e.into_inner()).clone()
}
