//! Middleware for HTTP observability
//!
//! Provides Tower middleware for metrics and tracing.

use std::time::Instant;
use std::future::Future;
use std::pin::Pin;
use std::task::{Context, Poll};
use std::sync::OnceLock;
use http::{Method, StatusCode, Response, Uri, Request};
use tracing::Span;

// Global metrics when prometheus feature is enabled
#[cfg(feature = "prometheus-metrics")]
use prometheus::{IntCounterVec, HistogramVec, Opts, HistogramOpts};

/// Metrics and tracing middleware for HTTP requests
///
/// This middleware:
/// - Records HTTP request metrics (counter, histogram)
/// - Adds distributed tracing to requests
/// - Tracks request duration
///
/// # Example
/// ```no_run
/// use backbone_observability::ObservabilityLayer;
/// use tower::ServiceBuilder;
///
/// let layer = ServiceBuilder::new()
///     .layer(ObservabilityLayer::new());
/// ```
#[derive(Clone)]
pub struct ObservabilityMiddleware<S> {
    inner: S,
}

impl<S> ObservabilityMiddleware<S> {
    /// Create a new observability middleware
    pub fn new(inner: S) -> Self {
        Self { inner }
    }
}

/// Layer for creating observability middleware
#[derive(Clone, Copy, Default)]
pub struct ObservabilityLayer;

impl ObservabilityLayer {
    /// Create a new observability layer
    pub fn new() -> Self {
        Self
    }
}

impl<S> tower::Layer<S> for ObservabilityLayer {
    type Service = ObservabilityMiddleware<S>;

    fn layer(&self, inner: S) -> Self::Service {
        ObservabilityMiddleware::new(inner)
    }
}

/// Response wrapper that records metrics
///
/// This wraps the inner service's Future and records metrics/tracing
/// when the response completes.
pub struct ResponseFuture<F> {
    inner: Pin<Box<F>>,
    start: Instant,
    method: Method,
    path: String,
    span: Span,
}

impl<F, Res, E> Future for ResponseFuture<F>
where
    F: Future<Output = Result<Res, E>>,
    Res: HttpStatus,
    E: std::fmt::Display,
{
    type Output = F::Output;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        // Poll the inner future
        match self.inner.as_mut().poll(cx) {
            Poll::Ready(Ok(response)) => {
                let duration = self.start.elapsed();

                // Extract status code from Response
                let status = response.status();

                // Record metrics
                record_http_metrics(
                    &self.method,
                    &self.path,
                    status,
                    duration.as_secs_f64(),
                );

                // Complete the span
                self.span.record("status", status.as_u16());
                self.span.record("duration_ms", duration.as_millis());
                self.span.in_scope(|| {
                    tracing::info!(
                        method = %self.method,
                        path = %self.path,
                        status = status.as_u16(),
                        duration_ms = duration.as_millis(),
                        "HTTP request completed"
                    );
                });

                Poll::Ready(Ok(response))
            }
            Poll::Ready(Err(e)) => {
                let duration = self.start.elapsed();

                // Record error metrics
                self.span.record("status", 500u16);
                self.span.record("duration_ms", duration.as_millis());
                self.span.in_scope(|| {
                    tracing::error!(
                        method = %self.method,
                        path = %self.path,
                        duration_ms = duration.as_millis(),
                        error = %e,
                        "HTTP request failed"
                    );
                });

                Poll::Ready(Err(e))
            }
            Poll::Pending => Poll::Pending,
        }
    }
}

// Helper trait to extract status from responses
///
/// This trait allows the middleware to extract HTTP status codes
/// from different response types in the Tower ecosystem.
pub trait HttpStatus {
    /// Returns the HTTP status code for this response
    fn status(&self) -> StatusCode;
}

impl<B> HttpStatus for Response<B> {
    fn status(&self) -> StatusCode {
        Response::status(self)
    }
}

impl<R, S> tower::Service<Request<R>> for ObservabilityMiddleware<S>
where
    R: Send + 'static,
    S: tower::Service<Request<R>> + Clone + Send + 'static,
    S::Future: Send + 'static,
    S::Response: HttpStatus,
    S::Error: std::error::Error + Send + Sync,
{
    type Response = S::Response;
    type Error = S::Error;
    type Future = Pin<Box<dyn Future<Output = Result<Self::Response, Self::Error>> + Send>>;

    fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.inner.poll_ready(cx)
    }

    fn call(&mut self, req: Request<R>) -> Self::Future {
        let method = req.method().clone();
        let uri = req.uri().clone();
        let path = extract_path_template(&uri);

        // Extract W3C trace context from incoming headers (traceparent / tracestate)
        #[cfg(feature = "otel-tracing")]
        let parent_cx = {
            struct HeaderExtractor<'a, B>(&'a Request<B>);
            impl<B> opentelemetry::propagation::Extractor for HeaderExtractor<'_, B> {
                fn get(&self, key: &str) -> Option<&str> {
                    self.0.headers().get(key).and_then(|v| v.to_str().ok())
                }
                fn keys(&self) -> Vec<&str> {
                    self.0.headers().keys().map(|k| k.as_str()).collect()
                }
            }
            opentelemetry::global::get_text_map_propagator(|prop| {
                prop.extract(&HeaderExtractor(&req))
            })
        };

        // Create a span for this request
        let span = tracing::info_span!(
            "http_request",
            method = %method,
            path = %path,
            status = tracing::field::Empty,
            duration_ms = tracing::field::Empty,
        );

        // Link span to the extracted parent trace context
        #[cfg(feature = "otel-tracing")]
        {
            use tracing_opentelemetry::OpenTelemetrySpanExt;
            span.set_parent(parent_cx);
        }

        let start = Instant::now();

        // Clone the inner service for the request
        let mut inner = self.inner.clone();

        Box::pin(async move {
            let _enter = span.enter();

            let response = inner.call(req).await?;

            // Record metrics after response is received
            let duration = start.elapsed();
            let status = response.status();

            record_http_metrics(
                &method,
                &path,
                status,
                duration.as_secs_f64(),
            );

            // Complete the span
            span.record("status", status.as_u16());
            span.record("duration_ms", duration.as_millis());

            Ok(response)
        })
    }
}

/// Extract a clean path from the URI
///
/// Removes query parameters and normalizes the path for metrics.
fn extract_path_template(uri: &Uri) -> String {
    let path = uri.path();

    // Normalize path segments
    // Replace IDs with placeholders for better metric aggregation
    let normalized = path
        .split('/')
        .map(|segment| {
            if segment.is_empty() {
                return String::new(); // Empty string for empty segments, join will handle the /
            }
            // Replace UUID-like segments with :id
            if looks_like_id(segment) {
                return ":id".to_string();
            }
            // Replace numeric segments with :num
            if segment.parse::<u64>().is_ok() {
                return ":num".to_string();
            }
            segment.to_string()
        })
        .collect::<Vec<_>>()
        .join("/");

    // Ensure path starts with /
    if !normalized.starts_with('/') {
        format!("/{}", normalized)
    } else {
        normalized
    }
}

/// Check if a segment looks like an ID
fn looks_like_id(s: &str) -> bool {
    // Check for UUID-like patterns (36 chars with 4 hyphens)
    if s.len() == 36 && s.chars().filter(|&c| c == '-').count() == 4 {
        return true;
    }
    // Check for MongoDB ObjectId (24 hex chars)
    if s.len() == 24 && s.chars().all(|c| c.is_ascii_hexdigit()) {
        return true;
    }
    // Check for hyphenated alphanumeric strings (like abc123-def456)
    let hyphen_count = s.chars().filter(|&c| c == '-').count();
    if hyphen_count > 0 && hyphen_count <= 3 {
        let alphanumeric_part: String = s.chars().filter(|c| c.is_alphanumeric()).collect();
        if alphanumeric_part.len() > 8 && alphanumeric_part.len() <= 24 {
            return true;
        }
    }
    // Check for alphanumeric strings that look like IDs (not common words, not purely numeric)
    // Threshold lowered to catch strings like "abc123def456" (12 chars)
    if s.len() >= 10 && s.len() <= 32 && s.chars().all(|c| c.is_alphanumeric()) {
        // Exclude purely numeric strings (handled separately as :num)
        if s.chars().all(|c| c.is_ascii_digit()) {
            return false;
        }
        // Exclude common path segments
        let lower = s.to_lowercase();
        if matches!(lower.as_str(), "api" | "v1" | "v2" | "admin" | "users" | "public" | "health" | "metrics") {
            return false;
        }
        return true;
    }
    false
}

/// Record HTTP request metrics
#[cfg(feature = "prometheus-metrics")]
fn record_http_metrics(method: &Method, path: &str, status: StatusCode, duration_secs: f64) {
    // Get or initialize metrics
    let counter = HTTP_REQUESTS_TOTAL.get_or_init(|| {
        let metric = IntCounterVec::new(
            Opts::new("http_requests_total", "Total number of HTTP requests"),
            &["method", "path", "status"]
        ).expect("Failed to create http_requests_total metric");
        let _ = prometheus::register(Box::new(metric.clone()));
        metric
    });

    let histogram = HTTP_REQUEST_DURATION_SECONDS.get_or_init(|| {
        let metric = HistogramVec::new(
            HistogramOpts::new("http_request_duration_seconds", "HTTP request latency in seconds")
                .buckets(vec![0.005, 0.01, 0.025, 0.05, 0.1, 0.25, 0.5, 1.0, 2.5, 5.0, 10.0])
                .into(),
            &["method", "path"],
        ).expect("Failed to create http_request_duration_seconds metric");
        let _ = prometheus::register(Box::new(metric.clone()));
        metric
    });

    // Record metrics
    let _ = counter
        .with_label_values(&[method.as_str(), path, &status.as_u16().to_string()])
        .inc();

    let _ = histogram
        .with_label_values(&[method.as_str(), path])
        .observe(duration_secs);
}

/// Global HTTP request counter metric
#[cfg(feature = "prometheus-metrics")]
static HTTP_REQUESTS_TOTAL: OnceLock<IntCounterVec> = OnceLock::new();

/// Global HTTP request duration histogram metric
#[cfg(feature = "prometheus-metrics")]
static HTTP_REQUEST_DURATION_SECONDS: OnceLock<HistogramVec> = OnceLock::new();

/// Stub for when prometheus-metrics is not enabled
#[cfg(not(feature = "prometheus-metrics"))]
fn record_http_metrics(method: &Method, path: &str, status: StatusCode, duration_secs: f64) {
    tracing::debug!(
        method = %method,
        path = %path,
        status = status.as_u16(),
        duration_secs = duration_secs,
        "HTTP request completed (metrics not recorded - enable prometheus-metrics feature)"
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_path_template() {
        let uri = Uri::from_static("/api/users/123456789012/orders/abc123-def456");
        let path = extract_path_template(&uri);
        assert_eq!(path, "/api/users/:num/orders/:id");
    }

    #[test]
    fn test_extract_path_with_query() {
        let uri = Uri::from_static("/api/users?page=2&limit=10");
        let path = extract_path_template(&uri);
        assert_eq!(path, "/api/users");
    }

    #[test]
    fn test_looks_like_id() {
        // UUID
        assert!(looks_like_id("550e8400-e29b-41d4-a716-446655440000"));
        // MongoDB ObjectId
        assert!(looks_like_id("507f1f77bcf86cd799439011"));
        // Alphanumeric string
        assert!(looks_like_id("abc123def456"));
        // Not an ID
        assert!(!looks_like_id("users"));
        assert!(!looks_like_id("admin"));
    }
}
