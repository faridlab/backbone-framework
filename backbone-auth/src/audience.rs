//! Tenant binding for session tokens (feature `axum`).
//!
//! In a database-per-tenant deployment every tenant verifies with the same signing key, and
//! tenant databases seeded from the same fixtures share row ids — so neither the signature nor
//! the acting unit tells tenants apart. The token itself has to say which tenant it was minted
//! for: the issuer writes the tenant into the standard `aud` claim
//! ([`OrgIssuer::for_audience`](crate::org::OrgIssuer::for_audience)), and a verifier bound to the
//! request's tenant ([`OrgVerifier::bind_audience`](crate::org::OrgVerifier::bind_audience),
//! [`CompanyVerifier::bind_audience`](crate::company::CompanyVerifier::bind_audience)) compares it
//! with the tenant the request was routed to.
//!
//! The binding reads the request's tenant through a function over the request extensions, so this
//! crate never depends on the router that resolves it. With `backbone-tenant`, pass
//! `backbone_tenant::axum_router::tenant_audience`.
//!
//! The rule a bound verifier applies:
//!
//! | request tenant | token `aud` | result |
//! |---|---|---|
//! | `t` | contains `t` | accepted |
//! | `t` | another tenant | refused |
//! | `t` | absent | refused, unless the deprecated unbound-token flag is on |
//! | none | absent | accepted (a surface that serves no tenant, e.g. the boot database) |
//! | none | present | refused (a tenant's token on a surface that serves none) |

use std::sync::Arc;

use axum::http::Extensions;
use serde::{Deserialize, Deserializer};

/// Reads the tenant a request was routed to, as the audience string a token must carry.
pub type AudienceSource = Arc<dyn Fn(&Extensions) -> Option<String> + Send + Sync>;

/// A verifier's tenant binding: where the request's tenant comes from, and whether tokens minted
/// before tokens carried a tenant are still accepted.
#[derive(Clone)]
pub(crate) struct AudienceBinding {
    pub(crate) source: AudienceSource,
    pub(crate) accept_unbound: bool,
}

impl AudienceBinding {
    pub(crate) fn new<F>(source: F) -> Self
    where
        F: Fn(&Extensions) -> Option<String> + Send + Sync + 'static,
    {
        Self {
            source: Arc::new(source),
            accept_unbound: false,
        }
    }

    /// Apply the binding rule to a token's audience for the request whose extensions are given.
    pub(crate) fn admits(&self, extensions: &Extensions, token_aud: &[String]) -> bool {
        admits(
            (self.source)(extensions).as_deref(),
            token_aud,
            self.accept_unbound,
        )
    }
}

/// The binding rule over an already-resolved request tenant (see the module table).
pub(crate) fn admits(
    request_tenant: Option<&str>,
    token_aud: &[String],
    accept_unbound: bool,
) -> bool {
    match request_tenant {
        Some(tenant) if token_aud.is_empty() => {
            if accept_unbound {
                tracing::warn!(
                    target: "backbone_auth::audience",
                    tenant,
                    "accepted a session token that names no tenant; this compatibility path is deprecated and will be removed"
                );
            }
            accept_unbound
        }
        Some(tenant) => token_aud.iter().any(|aud| aud == tenant),
        None => token_aud.is_empty(),
    }
}

/// `aud` is a string or an array of strings (RFC 7519 §4.1.3); either becomes a list, absent
/// becomes empty.
pub(crate) fn deserialize_audience<'de, D>(deserializer: D) -> Result<Vec<String>, D::Error>
where
    D: Deserializer<'de>,
{
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum OneOrMany {
        One(String),
        Many(Vec<String>),
    }
    Ok(match Option::<OneOrMany>::deserialize(deserializer)? {
        None => Vec::new(),
        Some(OneOrMany::One(aud)) => vec![aud],
        Some(OneOrMany::Many(auds)) => auds,
    })
}

#[cfg(test)]
mod tests {
    use super::admits;

    fn aud(values: &[&str]) -> Vec<String> {
        values.iter().map(|v| v.to_string()).collect()
    }

    #[test]
    fn the_binding_rule() {
        assert!(admits(Some("acme"), &aud(&["acme"]), false));
        assert!(admits(Some("acme"), &aud(&["other", "acme"]), false));
        assert!(!admits(Some("acme"), &aud(&["globex"]), false));
        assert!(!admits(Some("acme"), &aud(&[]), false));
        assert!(admits(Some("acme"), &aud(&[]), true));
        assert!(admits(None, &aud(&[]), false));
        assert!(!admits(None, &aud(&["acme"]), false));
        assert!(!admits(None, &aud(&["acme"]), true));
    }
}
