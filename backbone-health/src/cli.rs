//! In-binary healthcheck CLI helper.
//!
//! Distroless containers lack `curl` and a shell, so the standard
//! `HEALTHCHECK` directive in a Dockerfile cannot rely on external tooling.
//! Bake a `healthcheck` subcommand into your binary and call it from the
//! Dockerfile:
//!
//! ```dockerfile
//! HEALTHCHECK --interval=30s --timeout=5s --start-period=10s --retries=3 \
//!   CMD ["my-service", "healthcheck"]
//! ```
//!
//! ## Behavior
//!
//! Probes a URL with a 3 second timeout. Exits 0 on any 2xx response, returns
//! an error otherwise (which `main` should propagate as a non-zero exit
//! status).
//!
//! ## URL resolution
//!
//! Order of precedence:
//! 1. `HEALTHCHECK_URL` env var (if set and non-empty) — full URL.
//! 2. `http://127.0.0.1:<PORT>/health` where `<PORT>` comes from the `PORT`
//!    env var, falling back to the `default_port` argument.
//!
//! ## Quick start
//!
//! ```no_run
//! # async fn _main() -> anyhow::Result<()> {
//! // In your service's main():
//! let mut args = std::env::args().skip(1);
//! if let Some(cmd) = args.next() {
//!     match cmd.as_str() {
//!         "healthcheck" => {
//!             backbone_health::cli::run_healthcheck(3000).await?;
//!             return Ok(());
//!         }
//!         "serve" => {} // fall through
//!         other => anyhow::bail!("unknown subcommand: {}", other),
//!     }
//! }
//! # Ok(())
//! # }
//! ```

use std::time::Duration;

use crate::{HealthError, HealthResult};

/// Probe `HEALTHCHECK_URL` (or `http://127.0.0.1:<PORT>/health`) and exit 0
/// on 2xx, error otherwise.
///
/// `default_port` is used when the `PORT` env var is unset.
pub async fn run_healthcheck(default_port: u16) -> HealthResult<()> {
    let url = healthcheck_url(default_port);
    probe(&url, Duration::from_secs(3)).await
}

/// Probe an explicit URL with a custom timeout. Exposed for service-specific
/// scenarios (e.g. probing a sidecar or alternate path).
pub async fn probe(url: &str, timeout: Duration) -> HealthResult<()> {
    let client = reqwest::Client::builder()
        .timeout(timeout)
        .build()
        .map_err(|e| HealthError::Internal(format!("client build failed: {e}")))?;

    let resp = client
        .get(url)
        .send()
        .await
        .map_err(|e| HealthError::ServiceUnavailable(format!("probe request failed: {e}")))?;

    if resp.status().is_success() {
        Ok(())
    } else {
        Err(HealthError::ServiceUnavailable(format!(
            "healthcheck failed: HTTP {}",
            resp.status()
        )))
    }
}

/// Compute the effective healthcheck URL given a default port. Useful for
/// logging / diagnostics.
pub fn healthcheck_url(default_port: u16) -> String {
    if let Ok(url) = std::env::var("HEALTHCHECK_URL") {
        if !url.is_empty() {
            return url;
        }
    }
    let port = std::env::var("PORT").unwrap_or_else(|_| default_port.to_string());
    format!("http://127.0.0.1:{port}/health")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    static ENV_LOCK: Mutex<()> = Mutex::new(());

    #[test]
    fn url_uses_explicit_env_when_set() {
        let _g = ENV_LOCK.lock().unwrap();
        std::env::set_var("HEALTHCHECK_URL", "http://probe.local/x");
        let url = healthcheck_url(3000);
        std::env::remove_var("HEALTHCHECK_URL");
        assert_eq!(url, "http://probe.local/x");
    }

    #[test]
    fn url_falls_back_to_port_env_then_default() {
        let _g = ENV_LOCK.lock().unwrap();
        std::env::remove_var("HEALTHCHECK_URL");
        std::env::set_var("PORT", "9090");
        let url = healthcheck_url(3000);
        std::env::remove_var("PORT");
        assert_eq!(url, "http://127.0.0.1:9090/health");

        let url = healthcheck_url(3000);
        assert_eq!(url, "http://127.0.0.1:3000/health");
    }

    #[test]
    fn empty_healthcheck_url_does_not_override_port() {
        let _g = ENV_LOCK.lock().unwrap();
        std::env::set_var("HEALTHCHECK_URL", "");
        std::env::remove_var("PORT");
        let url = healthcheck_url(8080);
        std::env::remove_var("HEALTHCHECK_URL");
        assert_eq!(url, "http://127.0.0.1:8080/health");
    }
}
