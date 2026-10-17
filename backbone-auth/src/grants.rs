//! Permissions as data: the canonical name, the matching rules, the grants a person holds and the
//! source they are read from.
//!
//! A schema declares the permission *vocabulary* and what each route requires; who holds which
//! permission is data in the identity module's tables. This module is the framework half of that
//! split. It knows nothing of entities or tables:
//!
//! - **Canonical name.** `<resource>:<action>`, the resource an entity in snake_case
//!   (`payroll_entry`), the action `read`, `create`, `update`, `delete`, `restore`, `purge` or the
//!   name of a transition or custom verb (`payroll_entry:process`). Either half may be `*` in a
//!   *held* permission: `payroll_entry:*`, `*:read`, `*:*`.
//! - **Matching.** [`permission_matches`] decides whether one held permission covers one required
//!   one. [`AuthContext::require`](crate::middleware::AuthContext::require) applies it to
//!   `AuthContext.permissions` and refuses with [`PermissionRequired`].
//! - **Legacy spellings.** Three older spellings still read, and each use raises a deprecation
//!   signal ([`legacy_spellings_seen`], and a `tracing` warning once per spelling): `resource.action`
//!   (`buckets.read`), `<action>:<module>` (`write:payroll`) and the three-segment transition name
//!   `<model>:transition:<verb>` (`*:transition:*` covers a verb, never a generic read or write).
//! - **Where grants come from.** A [`GrantSource`] loads a person's [`Grants`]. With none installed,
//!   `AuthContext.permissions` is whatever the host's authentication put there, and `require`
//!   checks exactly that: nothing is ever allowed by default. Installing a resolver (see
//!   [`crate::resolver`], feature `axum`) replaces the source, once per request.
//!
//! `ADMIN` and `SUPERUSER` hold everything by name, and that authority is spelled in
//! [`Grants::effective`] as `*:*`, so a resolver-written `AuthContext` carries it as an ordinary
//! permission. `require` itself never grants by role name.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::{Mutex, OnceLock};

use async_trait::async_trait;
use uuid::Uuid;

/// The permission that matches everything.
pub const GOD_PERMISSION: &str = "*:*";

/// Role names that hold every permission by name.
pub const AUTHORITY_ROLES: [&str; 2] = ["SUPERUSER", "ADMIN"];

/// The violation code a refused permission answers with.
pub const PERMISSION_REQUIRED: &str = "permission_required";

/// The actions every entity's generic routes use. A legacy transition permission
/// (`<model>:transition:<verb>`) never covers one of these.
pub const GENERIC_ACTIONS: [&str; 6] = ["read", "create", "update", "delete", "restore", "purge"];

/// The caller lacks a permission a route requires.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PermissionRequired {
    /// The canonical permission the route requires.
    pub permission: String,
}

impl std::fmt::Display for PermissionRequired {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "the permission `{}` is required", self.permission)
    }
}

impl std::error::Error for PermissionRequired {}

/// Marks a request an outer gate has already admitted on a narrower rule than a permission (a
/// self-service lane serving a caller their own rows). A route's permission layer lets it pass.
///
/// Only code that can write request extensions can attach it, which is code the host owns: a
/// client cannot. Attach it only after the narrower rule has decided for *this* request.
#[derive(Debug, Clone, Copy, Default)]
pub struct PermissionWaiver;

/// How a held permission was spelled, when it is not the canonical `<resource>:<action>`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LegacySpelling {
    /// `resource.action`
    Dotted,
    /// `<action>:<module>`, `admin:*` or `admin:<module>`
    ActionFirst,
    /// `<model>:transition:<verb>`
    Transition,
    /// A plural resource name (`buckets:read` for `bucket:read`)
    Plural,
    /// A module-wide grant (`payroll:read` for an entity of the payroll module)
    ModuleGrain,
    /// The composite action `write` (`ticket:write`), which covered every mutation but a delete
    WriteAction,
}

impl LegacySpelling {
    /// A stable name for logs and counters.
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Dotted => "resource.action",
            Self::ActionFirst => "action:module",
            Self::Transition => "model:transition:verb",
            Self::Plural => "plural resource",
            Self::ModuleGrain => "module-wide grant",
            Self::WriteAction => "resource:write",
        }
    }
}

/// A held permission read into its parts.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Held {
    /// The resource half (`*` for any).
    pub resource: String,
    /// The action half (`*` for any).
    pub action: String,
    /// For a legacy transition name, the permission covers only verbs, never generic actions.
    pub verbs_only: bool,
    /// How it was spelled when that is not canonical.
    pub legacy: Option<LegacySpelling>,
}

impl Held {
    /// The canonical spelling.
    pub fn canonical(&self) -> String {
        format!("{}:{}", self.resource, self.action)
    }
}

fn is_name(s: &str) -> bool {
    !s.is_empty()
        && s.bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_')
}

fn is_half(s: &str) -> bool {
    s == "*" || is_name(s)
}

/// Read a held permission string. `None` for a string that is no permission in any spelling (it
/// then matches nothing).
///
/// | Spelling | Reads as |
/// |---|---|
/// | `payroll_entry:process`, `payroll_entry:*`, `*:read`, `*:*` | itself |
/// | `*` | `*:*` |
/// | `buckets.read` | `buckets:read` (legacy) |
/// | `*:transition:*`, `ticket:transition:resolve` | verbs of any or that model, never generic actions (legacy) |
/// | `admin:*` | `*:*` (legacy) |
/// | `admin:payroll` | `payroll:*` (legacy) |
/// | `write:payroll`, `read:payroll`, `delete:payroll` | `payroll:write` etc. (legacy) |
///
/// The action `write` is the composite the older vocabulary used: it covers `create`, `update`,
/// `restore` and every transition or verb, never `read`, `delete` or `purge` (see [`match_held`]).
pub fn read_held(raw: &str) -> Option<Held> {
    let raw = raw.trim();
    if raw == "*" {
        return Some(Held {
            resource: "*".into(),
            action: "*".into(),
            verbs_only: false,
            legacy: None,
        });
    }
    if !raw.contains(':') {
        let (resource, action) = raw.split_once('.')?;
        if is_half(resource) && is_half(action) {
            return Some(Held {
                resource: resource.into(),
                action: action.into(),
                verbs_only: false,
                legacy: Some(LegacySpelling::Dotted),
            });
        }
        return None;
    }
    let parts: Vec<&str> = raw.split(':').collect();
    match parts.as_slice() {
        [resource, "transition", verb] if is_half(resource) && is_half(verb) => Some(Held {
            resource: (*resource).into(),
            action: (*verb).into(),
            verbs_only: true,
            legacy: Some(LegacySpelling::Transition),
        }),
        ["admin", "*"] => Some(Held {
            resource: "*".into(),
            action: "*".into(),
            verbs_only: false,
            legacy: Some(LegacySpelling::ActionFirst),
        }),
        ["admin", module]
            if is_name(module) && !GENERIC_ACTIONS.contains(module) && *module != "write" =>
        {
            Some(Held {
                resource: (*module).into(),
                action: "*".into(),
                verbs_only: false,
                legacy: Some(LegacySpelling::ActionFirst),
            })
        }
        [action, module] if matches!(*action, "read" | "write" | "delete") && is_name(module) => {
            Some(Held {
                resource: (*module).into(),
                action: (*action).into(),
                verbs_only: false,
                legacy: Some(LegacySpelling::ActionFirst),
            })
        }
        [resource, action] if is_half(resource) && is_half(action) => Some(Held {
            resource: (*resource).into(),
            action: (*action).into(),
            verbs_only: false,
            legacy: None,
        }),
        _ => None,
    }
}

/// The canonical spelling of a stored permission, or `None` when it is no permission.
pub fn normalise(raw: &str) -> Option<String> {
    read_held(raw).map(|h| h.canonical())
}

/// The plural the schema generator gives a snake_case entity name: `ies` for a final `y` after a
/// consonant, `es` after `s`, `x`, `ch` or `sh`, else `s`.
pub fn plural_of(singular: &str) -> String {
    let vowel = |c: char| matches!(c, 'a' | 'e' | 'i' | 'o' | 'u');
    if let Some(stem) = singular.strip_suffix('y') {
        if stem.chars().last().is_some_and(|c| !vowel(c)) {
            return format!("{stem}ies");
        }
    }
    if singular.ends_with('s')
        || singular.ends_with('x')
        || singular.ends_with("ch")
        || singular.ends_with("sh")
    {
        return format!("{singular}es");
    }
    format!("{singular}s")
}

/// Whether `held` covers `required` (a canonical `<resource>:<action>`), and how the grant was
/// spelled: `Some(None)` for a canonical match, `Some(Some(spelling))` for a legacy one, `None` for
/// no match.
pub fn match_held(held: &Held, required: &str) -> Option<Option<LegacySpelling>> {
    let (rr, ra) = required.split_once(':')?;
    if held.verbs_only && GENERIC_ACTIONS.contains(&ra) {
        return None;
    }
    let write_alias =
        held.action == "write" && ra != "write" && !matches!(ra, "read" | "delete" | "purge");
    if !(held.action == "*" || held.action == ra || write_alias) {
        return None;
    }
    let legacy = if write_alias {
        held.legacy.or(Some(LegacySpelling::WriteAction))
    } else {
        held.legacy
    };
    if held.resource == "*" || held.resource == rr {
        return Some(legacy);
    }
    // A plural resource name still reads for one release.
    if held.resource == plural_of(rr) {
        return Some(legacy.or(Some(LegacySpelling::Plural)));
    }
    None
}

/// Whether the one held permission string covers `required`.
pub fn permission_matches(held: &str, required: &str) -> bool {
    read_held(held).is_some_and(|h| match_held(&h, required).is_some())
}

/// Every spelling a legacy reader accepted, keyed by the stored string, with its canonical form.
static SEEN: OnceLock<Mutex<BTreeMap<String, String>>> = OnceLock::new();
const SEEN_CAP: usize = 4096;

/// Raise the deprecation signal for a legacy spelling that granted access: a `tracing` warning, once
/// per stored string, and a record [`legacy_spellings_seen`] reports.
pub fn note_legacy(held: &str, canonical: &str, spelling: LegacySpelling) {
    let map = SEEN.get_or_init(|| Mutex::new(BTreeMap::new()));
    let Ok(mut map) = map.lock() else { return };
    if map.len() >= SEEN_CAP || map.contains_key(held) {
        return;
    }
    map.insert(held.to_string(), canonical.to_string());
    tracing::warn!(
        target: "backbone::auth",
        stored = held,
        canonical,
        spelling = spelling.as_str(),
        "a legacy permission spelling granted access; rewrite the stored permission (see the permission migration guide)"
    );
}

/// The legacy permission strings that have granted access in this process, with the canonical form
/// each should be rewritten to.
pub fn legacy_spellings_seen() -> Vec<(String, String)> {
    SEEN.get()
        .and_then(|m| {
            m.lock()
                .ok()
                .map(|m| m.iter().map(|(k, v)| (k.clone(), v.clone())).collect())
        })
        .unwrap_or_default()
}

/// Check `required` (and the `alternates` a route also accepts, themselves legacy) against the held
/// permission strings. A canonical match wins over a legacy one so a person holding both raises no
/// signal.
pub fn check_held<'a>(
    held: impl IntoIterator<Item = &'a String> + Clone,
    required: &str,
    alternates: &[&str],
) -> bool {
    let mut legacy_hit: Option<(String, String, LegacySpelling)> = None;
    for raw in held.clone() {
        let Some(h) = read_held(raw) else { continue };
        if let Some(how) = match_held(&h, required) {
            match how {
                None => return true,
                Some(spelling) => {
                    legacy_hit.get_or_insert((raw.clone(), h.canonical(), spelling));
                }
            }
        }
    }
    if legacy_hit.is_none() {
        for alt in alternates {
            for raw in held.clone() {
                let Some(h) = read_held(raw) else { continue };
                if match_held(&h, alt).is_some() {
                    legacy_hit.get_or_insert((
                        raw.clone(),
                        h.canonical(),
                        h.legacy.unwrap_or(LegacySpelling::ModuleGrain),
                    ));
                }
            }
        }
    }
    match legacy_hit {
        Some((raw, canonical, spelling)) => {
            note_legacy(&raw, &canonical, spelling);
            true
        }
        None => false,
    }
}

/// The roles and permissions one person holds, in one tenant.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Grants {
    /// Role names held.
    pub roles: BTreeSet<String>,
    /// Permission strings held, as stored.
    pub permissions: BTreeSet<String>,
}

impl Grants {
    /// Grants from role names and permission strings.
    pub fn new<R, P>(roles: R, permissions: P) -> Self
    where
        R: IntoIterator,
        R::Item: Into<String>,
        P: IntoIterator,
        P::Item: Into<String>,
    {
        Self {
            roles: roles.into_iter().map(Into::into).collect(),
            permissions: permissions.into_iter().map(Into::into).collect(),
        }
    }

    /// Holds everything: an authority role by name, or the god permission.
    pub fn is_authority(&self) -> bool {
        AUTHORITY_ROLES.iter().any(|r| self.roles.contains(*r))
            || self.permissions.contains(GOD_PERMISSION)
    }

    /// The permission list written into `AuthContext.permissions` and published to a client: what is
    /// held, with authority spelled as `*:*`, because that is what it means to every check.
    pub fn effective(&self) -> Vec<String> {
        let mut out = self.permissions.clone();
        if self.is_authority() {
            out.insert(GOD_PERMISSION.to_string());
        }
        out.into_iter().collect()
    }

    /// Whether the holder may do what `required` (a canonical `<resource>:<action>`) says.
    pub fn allows(&self, required: &str) -> bool {
        self.is_authority() || check_held(self.permissions.iter(), required, &[])
    }
}

/// A grant source could not answer. A caller of it must treat that as "no access", never as "no
/// grants held, carry on".
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GrantError(pub String);

impl std::fmt::Display for GrantError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "grants could not be loaded: {}", self.0)
    }
}

impl std::error::Error for GrantError {}

/// Where a person's grants are read from: the identity module's tables in a composed service, a
/// fixed table in a test.
///
/// Install one with [`crate::resolver::GrantResolver`] (feature `axum`). A service that installs
/// none keeps whatever `AuthContext.permissions` its authentication produced; no source means no
/// grants, never every grant.
#[async_trait]
pub trait GrantSource: Send + Sync {
    /// The grants of `user` while acting in `unit` (the org unit or company the session acts at).
    ///
    /// Run on the request's bound connection when the host binds one, which is what lets fenced
    /// membership rows be read. An error means the store could not be read: the request is refused.
    async fn load(&self, user: Uuid, unit: Option<Uuid>) -> Result<Grants, GrantError>;

    /// [`load`](Self::load) with the request's extensions, for a source that reads the tenant's own
    /// pool off them. Defaults to `load`.
    #[cfg(feature = "axum")]
    async fn load_for_request(
        &self,
        user: Uuid,
        unit: Option<Uuid>,
        _extensions: &axum::http::Extensions,
    ) -> Result<Grants, GrantError> {
        self.load(user, unit).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn held(list: &[&str]) -> Vec<String> {
        list.iter().map(|s| s.to_string()).collect()
    }

    fn allows(list: &[&str], required: &str) -> bool {
        check_held(held(list).iter(), required, &[])
    }

    #[test]
    fn a_held_permission_covers_exactly_its_resource_and_action() {
        assert!(allows(&["ticket:read"], "ticket:read"));
        assert!(!allows(&["ticket:read"], "ticket:update"));
        assert!(!allows(&["ticket:read"], "project:read"));
        assert!(!allows(&[], "ticket:read"));
        assert!(
            !allows(&["ticket"], "ticket:read"),
            "no colon, no dot: no permission"
        );
    }

    #[test]
    fn wildcards_cover_a_half_or_everything() {
        assert!(allows(&["ticket:*"], "ticket:resolve"));
        assert!(!allows(&["ticket:*"], "project:read"));
        assert!(allows(&["*:read"], "project:read"));
        assert!(allows(&["*:read"], "ticket:read"));
        assert!(!allows(&["*:read"], "ticket:update"));
        assert!(allows(&["*:*"], "anything:at_all"));
        assert!(allows(&["*"], "ticket:read"), "a bare star reads as *:*");
    }

    #[test]
    fn a_dotted_permission_reads_as_resource_colon_action_with_a_signal() {
        assert_eq!(normalise("buckets.read").as_deref(), Some("buckets:read"));
        assert!(allows(&["ticket.read"], "ticket:read"));
        assert!(allows(&["ticket.*"], "ticket:update"));
        assert!(legacy_spellings_seen()
            .iter()
            .any(|(k, v)| k == "ticket.read" && v == "ticket:read"));
    }

    #[test]
    fn an_action_first_permission_reads_as_module_colon_action() {
        assert_eq!(normalise("write:payroll").as_deref(), Some("payroll:write"));
        assert_eq!(normalise("admin:*").as_deref(), Some("*:*"));
        assert_eq!(normalise("admin:payroll").as_deref(), Some("payroll:*"));
        assert!(allows(&["admin:*"], "ticket:read"));
        // `admin:<action>` is an entity named admin, not a module grant.
        assert_eq!(normalise("admin:read").as_deref(), Some("admin:read"));
    }

    #[test]
    fn a_legacy_transition_permission_covers_verbs_never_generic_actions() {
        assert!(allows(&["ticket:transition:resolve"], "ticket:resolve"));
        assert!(allows(&["ticket:transition:*"], "ticket:resolve"));
        assert!(allows(&["*:transition:*"], "ticket:resolve"));
        assert!(
            !allows(&["*:transition:*"], "ticket:read"),
            "a verb grant must not open reads"
        );
        assert!(!allows(&["*:transition:*"], "ticket:update"));
        assert!(!allows(&["*:transition:*"], "ticket:delete"));
        assert!(!allows(&["project:transition:*"], "ticket:resolve"));
        assert!(!allows(&["ticket:transition:resolve"], "ticket:archive"));
    }

    #[test]
    fn a_plural_resource_still_reads_for_one_release() {
        assert!(allows(&["tickets:read"], "ticket:read"));
        assert!(allows(&["categories:read"], "category:read"));
        assert!(allows(&["addresses:read"], "address:read"));
        assert!(!allows(&["tickets:read"], "ticket:update"));
        assert!(!allows(&["ticketss:read"], "ticket:read"));
    }

    #[test]
    fn a_route_may_also_accept_the_module_grain_spelling() {
        let list = held(&["payroll:read"]);
        assert!(!check_held(list.iter(), "salary_slip:read", &[]));
        assert!(check_held(
            list.iter(),
            "salary_slip:read",
            &["payroll:read"]
        ));
        let list = held(&["write:payroll"]);
        assert!(check_held(
            list.iter(),
            "salary_slip:update",
            &["payroll:write"]
        ));
        assert!(!check_held(
            list.iter(),
            "salary_slip:update",
            &["payroll:read"]
        ));
    }

    #[test]
    fn the_composite_write_action_covers_mutations_and_verbs_only() {
        assert!(allows(&["ticket:write"], "ticket:update"));
        assert!(allows(&["ticket:write"], "ticket:create"));
        assert!(allows(&["ticket:write"], "ticket:restore"));
        assert!(allows(&["ticket:write"], "ticket:resolve"));
        assert!(!allows(&["ticket:write"], "ticket:read"));
        assert!(!allows(&["ticket:write"], "ticket:delete"));
        assert!(!allows(&["ticket:write"], "ticket:purge"));
        assert!(
            allows(&["tickets:write"], "ticket:update"),
            "collection grain reads too"
        );
    }

    #[test]
    fn junk_is_no_permission() {
        for junk in [
            "",
            ":",
            "a:b:c",
            "A:B",
            "ticket:",
            ":read",
            "a b:c",
            "ticket.read.x",
        ] {
            assert!(read_held(junk).is_none(), "{junk:?}");
            assert!(!allows(&[junk], "ticket:read"), "{junk:?}");
        }
    }

    #[test]
    fn authority_is_a_role_by_name_or_the_god_permission_and_spells_as_a_star_pair() {
        for g in [
            Grants::new(["ADMIN"], Vec::<String>::new()),
            Grants::new(["SUPERUSER"], Vec::<String>::new()),
            Grants::new(["X"], ["*:*"]),
        ] {
            assert!(g.is_authority());
            assert!(g.allows("anything:at_all"));
            assert_eq!(g.effective(), vec!["*:*".to_string()]);
        }
        let g = Grants::new(["CHART_ADMIN"], Vec::<String>::new());
        assert!(!g.is_authority());
        assert!(!g.allows("account:read"));
        assert!(g.effective().is_empty());
    }

    #[test]
    fn the_plural_follows_the_generators_rule() {
        assert_eq!(plural_of("ticket"), "tickets");
        assert_eq!(plural_of("category"), "categories");
        assert_eq!(plural_of("day"), "days");
        assert_eq!(plural_of("address"), "addresses");
        assert_eq!(plural_of("box"), "boxes");
    }
}
