//! Simple health server implementation

use super::*;
use serde_json;
use std::io::{Write, BufReader, BufWriter, BufRead};
use std::net::TcpStream;
use std::sync::Arc;
use std::time::Duration;

/// Simple health server that provides basic HTTP endpoints
#[derive(Debug)]
pub struct SimpleHealthServer {
    checker: Arc<HealthChecker>,
    port: u16,
    host: String,
}

impl SimpleHealthServer {
    /// Create a new simple health server
    pub fn new(checker: HealthChecker, port: u16) -> Self {
        Self {
            checker: Arc::new(checker),
            port,
            host: "127.0.0.1".to_string(),
        }
    }

    /// Set the host address
    pub fn host(mut self, host: String) -> Self {
        self.host = host;
        self
    }

    /// Get the current health status as JSON
    pub async fn health_json(&self) -> String {
        let status = self.checker.health_status().await;
        serde_json::to_string(&status).unwrap_or_default()
    }

    /// Get detailed health report as JSON
    pub async fn detailed_health_json(&self) -> String {
        let report = self.checker.health_report().await;
        serde_json::to_string(&report).unwrap_or_default()
    }

    /// Get readiness status as JSON
    pub async fn readiness_json(&self) -> String {
        let readiness = self.checker.readiness().await;
        serde_json::to_string(&readiness).unwrap_or_default()
    }

    /// Get liveness status as JSON
    pub async fn liveness_json(&self) -> String {
        let liveness = self.checker.liveness(None).await;
        serde_json::to_string(&liveness).unwrap_or_default()
    }

    /// Start a basic HTTP server using std::net
    pub async fn start_basic_server(&self) -> HealthResult<()> {
        use std::net::TcpListener;
        

        let listener = TcpListener::bind(format!("{}:{}", self.host, self.port))
            .map_err(|e| HealthError::Internal(format!("Failed to bind to port {}: {}", self.port, e)))?;

        println!("Health server listening on {}:{}", self.host, self.port);

        // Simple request handler loop
        for stream in listener.incoming() {
            match stream {
                Ok(stream) => {
                    let checker = Arc::clone(&self.checker);
                    tokio::spawn(async move {
                        if let Err(e) = Self::handle_request(stream, checker).await {
                            eprintln!("Error handling request: {}", e);
                        }
                    });
                }
                Err(e) => {
                    eprintln!("Error accepting connection: {}", e);
                }
            }
        }

        Ok(())
    }

    /// Handle a single HTTP request
    async fn handle_request(stream: TcpStream, checker: Arc<HealthChecker>) -> HealthResult<()> {
        let mut reader = BufReader::new(&stream);
        let mut writer = BufWriter::new(&stream);

        // Read request line
        let mut request_line = String::new();
        reader.read_line(&mut request_line).map_err(|e| {
            HealthError::Internal(format!("Failed to read request: {}", e))
        })?;

        let request_line = request_line.trim();
        if request_line.is_empty() {
            return Ok(());
        }

        // Parse request
        let parts: Vec<&str> = request_line.split_whitespace().collect();
        if parts.len() < 2 {
            return Ok(());
        }

        let method = parts[0];
        let path = parts[1];

        // Only handle GET requests
        if method != "GET" {
            Self::send_response(&mut writer, StatusCode::MethodNotAllowed, "Method Not Allowed");
            return Ok(());
        }

        // Route request
        match path {
            "/health" | "/" => {
                let health_json = checker.health_status().await;
                let response = serde_json::to_string(&health_json).unwrap_or_default();
                Self::send_response(&mut writer, StatusCode::Ok, &response);
            }
            "/health/detailed" => {
                let report_json = checker.health_report().await;
                let response = serde_json::to_string(&report_json).unwrap_or_default();
                Self::send_response(&mut writer, StatusCode::Ok, &response);
            }
            "/ready" => {
                let readiness_json = checker.readiness().await;
                let response = serde_json::to_string(&readiness_json).unwrap_or_default();
                let status_code = match serde_json::from_str::<SimpleHealthResponse>(&response)
                    .map(|r| r.status)
                    .unwrap_or(HealthStatus::Unhealthy) {
                        HealthStatus::Healthy => StatusCode::Ok,
                        HealthStatus::Degraded | HealthStatus::Unhealthy => StatusCode::ServiceUnavailable,
                    };
                Self::send_response(&mut writer, status_code, &response);
            }
            "/live" => {
                let liveness_json = checker.liveness(None).await;
                let response = serde_json::to_string(&liveness_json).unwrap_or_default();
                Self::send_response(&mut writer, StatusCode::Ok, &response);
            }
            _ => {
                Self::send_response(&mut writer, StatusCode::NotFound, "Not Found");
            }
        }

        Ok(())
    }

    /// Send HTTP response
    fn send_response(writer: &mut BufWriter<&TcpStream>, status: StatusCode, body: &str) {
        let status_text = match status {
            StatusCode::Ok => "200 OK",
            StatusCode::NotFound => "404 Not Found",
            StatusCode::MethodNotAllowed => "405 Method Not Allowed",
            StatusCode::ServiceUnavailable => "503 Service Unavailable",
        };

        let response = format!(
            "HTTP/1.1 {}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nAccess-Control-Allow-Origin: *\r\n\r\n{}",
            status_text,
            body.len(),
            body
        );

        let _ = writer.write_all(response.as_bytes());
        let _ = writer.flush();
    }

    /// Run a simple health check demo
    pub async fn run_demo() -> HealthResult<()> {
        println!("🏥 Backbone Health Server Demo");
        println!("===========================");

        // Create health checker with mock components
        let mut config = HealthConfig::default();
        config.app_name = Some("Backbone Demo".to_string());
        config.app_version = Some("2.0.0".to_string());

        let checker = HealthChecker::new(config);

        // Add mock components
        let db_check = MockHealthCheck::healthy("database".to_string());
        checker.add_component("database".to_string(), Box::new(db_check)).await?;

        let cache_check = MockHealthCheck::healthy("cache".to_string());
        checker.add_component("cache".to_string(), Box::new(cache_check)).await?;

        let api_check = CustomHealthCheck::new(
            "api".to_string(),
            || async {
                let mut status = ComponentStatus::new("api".to_string());
                status.record_success(Duration::from_millis(15));
                status.add_metadata("version".to_string(), "1.0.0".to_string());
                Ok(status)
            }
        );
        checker.add_component("api".to_string(), Box::new(api_check)).await?;

        // Start the health checker
        checker.start().await?;

        // Create health server
        let server = SimpleHealthServer::new(checker, 8080);

        // Show available endpoints
        println!("🚀 Health server starting on http://localhost:8080");
        println!();
        println!("📊 Available endpoints:");
        println!("   GET /health              - Basic health status");
        println!("   GET /health/detailed     - Detailed health report");
        println!("   GET /ready               - Readiness probe (Kubernetes)");
        println!("   GET /live                - Liveness probe (Kubernetes)");
        println!();
        println!("🔍 Try these commands:");
        println!("   curl http://localhost:8080/health");
        println!("   curl http://localhost:8080/health/detailed");
        println!("   curl http://localhost:8080/ready");
        println!("   curl http://localhost:8080/live");
        println!();
        println!("Press Ctrl+C to stop the server");

        // Start the server
        server.start_basic_server().await
    }
}

/// Simple HTTP status codes
#[derive(Debug, Clone, Copy)]
pub enum StatusCode {
    Ok = 200,
    NotFound = 404,
    MethodNotAllowed = 405,
    ServiceUnavailable = 503,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_simple_health_server_creation() {
        let config = HealthConfig::default();
        let checker = HealthChecker::new(config);
        let server = SimpleHealthServer::new(checker, 8080);

        assert_eq!(server.port, 8080);
        assert_eq!(server.host, "127.0.0.1");
    }

    #[tokio::test]
    async fn test_health_json_generation() {
        let checker = HealthChecker::new(HealthConfig::default());
        let server = SimpleHealthServer::new(checker, 8080);

        let health_json = server.health_json().await;
        assert!(!health_json.is_empty());

        // Parse and verify the JSON
        let health_response: SimpleHealthResponse = serde_json::from_str(&health_json).unwrap();
        assert_eq!(health_response.status, HealthStatus::Healthy);
    }

    #[tokio::test]
    async fn test_detailed_health_json_generation() {
        let checker = HealthChecker::new(HealthConfig::default());
        let server = SimpleHealthServer::new(checker, 8080);

        let detailed_json = server.detailed_health_json().await;
        assert!(!detailed_json.is_empty());

        // Parse and verify the JSON
        let health_report: HealthReport = serde_json::from_str(&detailed_json).unwrap();
        assert_eq!(health_report.status, HealthStatus::Healthy);
    }
}