//! Tenant resolution from the incoming host (ADR-0027 decision 4).
//!
//! The subdomain is the primary tenant signal: `{slug}.<product-domain>`. An optional custom-domain
//! map answers for enterprise domains (an additive row in the control plane's registry, never a
//! migration), and development environments may override resolution by header — a switch the
//! resolver must be explicitly built with, so a production profile can never be talked into
//! resolving a tenant the host did not name.
//!
//! Slug validation follows the same law the provisioner enforces for database names
//! ([`crate::provision`]): a tenant slug must be a bounded run of `[a-z0-9-]` starting with a
//! letter, because the slug becomes an identifier (a database name, cache keys) downstream.

use std::collections::HashMap;
use std::sync::Arc;

use crate::TenantId;

/// Longest slug accepted — `tenant_` + slug must stay within Postgres' 63-char identifier limit,
/// and no real subdomain is anywhere near this long.
const MAX_SLUG_LEN: usize = 48;

/// Why a request's tenant could not be resolved.
#[derive(Debug, thiserror::Error, PartialEq)]
pub enum ResolveError {
    /// The host IS the product domain (its apex) — no subdomain carries a tenant signal. Typically
    /// the marketing/landing host; the composer serves it outside the tenant router.
    #[error("host '{host}' is the product domain itself; no tenant subdomain present")]
    ApexHost { host: String },
    /// The host is not under the product domain and the custom-domain map does not know it.
    #[error("host '{host}' matches no tenant: not a subdomain of the product domain and not in the custom-domain map")]
    UnknownHost { host: String },
    /// The subdomain parsed out of the host is not a valid tenant slug.
    #[error("subdomain '{slug}' is not a valid tenant slug: {reason}")]
    BadSlug { slug: String, reason: &'static str },
    /// The dev header override was supplied but the resolver is not built to honor it.
    #[error("a tenant header override was supplied but header overrides are disabled on this resolver")]
    OverrideDisabled,
}

/// Whether a tenant slug is well-formed: `[a-z0-9-]`, starts with a letter, bounded — the same
/// law the provisioner applies when deriving a database name, checked at the door so every
/// resolved [`TenantId`] is safe to interpolate downstream.
pub fn valid_slug(slug: &str) -> Result<(), ResolveError> {
    let reject = |reason: &'static str| ResolveError::BadSlug { slug: slug.to_string(), reason };
    if slug.is_empty() {
        return Err(reject("empty"));
    }
    if !slug.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-') {
        return Err(reject("only lowercase letters, digits and '-' are allowed"));
    }
    let first = slug.chars().next().unwrap();
    if !first.is_ascii_lowercase() {
        return Err(reject("must start with a lowercase letter"));
    }
    if slug.len() > MAX_SLUG_LEN {
        return Err(reject("too long"));
    }
    Ok(())
}

/// Answers "which tenant does this custom domain belong to?". The control plane's registry
/// implements this over its database; anything config-sourced can use [`StaticDomainMap`].
#[async_trait::async_trait]
pub trait DomainMap: Send + Sync {
    /// The tenant owning `host` (exact, lowercase), if the map knows it.
    async fn tenant_for(&self, host: &str) -> Option<TenantId>;
}

/// An in-memory custom-domain map — the config-sourced shape (a deployment without the control
/// plane, or the seed rows before the registry service exists).
#[derive(Debug, Clone, Default)]
pub struct StaticDomainMap(HashMap<String, TenantId>);

impl StaticDomainMap {
    /// An empty map.
    pub fn new() -> Self {
        Self(HashMap::new())
    }

    /// Register `domain` (any case; stored lowercase) as belonging to `tenant`.
    pub fn with(mut self, domain: &str, tenant: &str) -> Self {
        self.0.insert(domain.to_ascii_lowercase(), TenantId::from(tenant));
        self
    }
}

#[async_trait::async_trait]
impl DomainMap for StaticDomainMap {
    async fn tenant_for(&self, host: &str) -> Option<TenantId> {
        self.0.get(host).cloned()
    }
}

/// Resolves request hosts to tenants.
///
/// Built from the product domain (e.g. `app.example.com`); resolution order per ADR-0027:
///
/// 1. the dev header override — only when [`Self::allow_header_override`] was set (development
///    profiles; a production resolver ignores the header rather than trusting it);
/// 2. `{slug}.` + the product domain — the subdomain is the tenant slug;
/// 3. the custom-domain map, for enterprise domains.
#[derive(Clone)]
pub struct HostResolver {
    base_domain: String,
    custom_domains: Option<Arc<dyn DomainMap>>,
    allow_header_override: bool,
}

impl HostResolver {
    /// Resolve `{slug}.{base_domain}` hosts. `base_domain` is stored lowercase, without port.
    pub fn new(base_domain: &str) -> Self {
        Self {
            base_domain: base_domain.to_ascii_lowercase(),
            custom_domains: None,
            allow_header_override: false,
        }
    }

    /// Consult `map` for hosts outside the product domain (custom enterprise domains).
    pub fn with_custom_domains(mut self, map: Arc<dyn DomainMap>) -> Self {
        self.custom_domains = Some(map);
        self
    }

    /// Honor the dev header override. Off by default; enable only on development profiles —
    /// when off, a supplied override header is an error, not a silent ignore, so a production
    /// misconfiguration is visible in the response instead of quietly bypassing host resolution.
    pub fn allow_header_override(mut self, yes: bool) -> Self {
        self.allow_header_override = yes;
        self
    }

    /// Resolve `host_header` (the request's `Host:` value; a port suffix is tolerated) to a
    /// tenant, consulting `override_header` first when overrides are enabled.
    pub async fn resolve(
        &self,
        host_header: &str,
        override_header: Option<&str>,
    ) -> Result<TenantId, ResolveError> {
        let host = normalize_host(host_header);

        if let Some(slug) = override_header {
            if self.allow_header_override {
                let slug = slug.trim().to_ascii_lowercase();
                valid_slug(&slug)?;
                return Ok(TenantId::from(slug));
            }
            return Err(ResolveError::OverrideDisabled);
        }

        if host == self.base_domain {
            return Err(ResolveError::ApexHost { host });
        }
        if let Some(slug) = host.strip_suffix(&format!(".{}", self.base_domain)) {
            valid_slug(slug)?;
            return Ok(TenantId::from(slug));
        }
        if let Some(map) = &self.custom_domains {
            if let Some(tenant) = map.tenant_for(&host).await {
                return Ok(tenant);
            }
        }
        Err(ResolveError::UnknownHost { host })
    }
}

/// Lowercase a Host header value and strip any `:port` suffix. IPv6 literals keep their brackets
/// (they never match a product domain; the custom-domain map sees them verbatim).
fn normalize_host(host: &str) -> String {
    let h = host.trim().to_ascii_lowercase();
    match h.rsplit_once(':') {
        // A colon with only digits after it is a port; anything else is an IPv6 literal.
        Some((head, tail)) if !tail.is_empty() && tail.chars().all(|c| c.is_ascii_digit()) => {
            head.to_string()
        }
        _ => h,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn subdomain_of_the_product_domain_resolves_to_a_slug() {
        let r = HostResolver::new("app.example.com");
        assert_eq!(
            r.resolve("acme.app.example.com", None).await.unwrap(),
            TenantId::from("acme")
        );
        // Case and port are normalized away.
        assert_eq!(
            r.resolve("ACME.App.Example.COM:8443", None).await.unwrap(),
            TenantId::from("acme")
        );
    }

    #[tokio::test]
    async fn apex_host_has_no_tenant_signal() {
        let r = HostResolver::new("app.example.com");
        assert_eq!(
            r.resolve("app.example.com", None).await,
            Err(ResolveError::ApexHost { host: "app.example.com".into() })
        );
    }

    #[tokio::test]
    async fn malformed_subdomains_are_rejected_not_routed() {
        let r = HostResolver::new("app.example.com");
        // Mixed case is absent from this list on purpose: hosts normalize to lowercase before the
        // slug is checked, so `Acme.…` is tenant `acme`, not a rejection.
        for bad in ["1acme.app.example.com", "a_b.app.example.com", "-acme.app.example.com", "acme!.app.example.com"] {
            assert!(r.resolve(bad, None).await.is_err(), "{bad} must not resolve");
        }
    }

    #[tokio::test]
    async fn unknown_hosts_fail_closed() {
        let r = HostResolver::new("app.example.com");
        assert_eq!(
            r.resolve("elsewhere.example.org", None).await,
            Err(ResolveError::UnknownHost { host: "elsewhere.example.org".into() })
        );
    }

    #[tokio::test]
    async fn custom_domains_map_to_their_tenant() {
        let map = StaticDomainMap::new().with("erp.acme-corp.example", "acme-corp");
        let r = HostResolver::new("app.example.com").with_custom_domains(Arc::new(map));
        assert_eq!(
            r.resolve("erp.acme-corp.example", None).await.unwrap(),
            TenantId::from("acme-corp")
        );
        // The map is exact-match: a subdomain of a custom domain is NOT the custom domain.
        assert!(r.resolve("www.erp.acme-corp.example", None).await.is_err());
    }

    #[tokio::test]
    async fn header_override_only_when_enabled() {
        let strict = HostResolver::new("app.example.com");
        assert_eq!(
            strict.resolve("acme.app.example.com", Some("other")).await,
            Err(ResolveError::OverrideDisabled)
        );
        let dev = HostResolver::new("app.example.com").allow_header_override(true);
        assert_eq!(
            dev.resolve("localhost:3000", Some("acme")).await.unwrap(),
            TenantId::from("acme")
        );
        // The override carries the same slug law — no injection-shaped tenant ids.
        assert!(dev.resolve("localhost", Some("acme'; drop")).await.is_err());
    }
}
