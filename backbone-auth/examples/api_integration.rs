//! API integration example for backbone-auth
//! Demonstrates web framework integration patterns with Actix Web, Axum, and Warp

use backbone_auth::*;
use uuid::Uuid;
use serde::{Serialize, Deserialize};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

// Common types for all web frameworks
#[derive(Debug, Serialize, Deserialize)]
struct LoginRequest {
    pub email_or_username: String,
    pub password: String,
    pub remember_me: Option<bool>,
}

#[derive(Debug, Serialize, Deserialize)]
struct RegisterRequest {
    pub email: String,
    pub password: String,
    pub username: String,
}

#[derive(Debug, Serialize, Deserialize)]
struct AuthResponse {
    pub success: bool,
    pub user_id: Option<String>,
    pub token: Option<String>,
    pub refresh_token: Option<String>,
    pub expires_at: Option<String>,
    pub requires_2fa: bool,
    pub message: String,
}

#[derive(Debug, Serialize, Deserialize)]
struct UserResponse {
    pub id: String,
    pub email: String,
    pub roles: Vec<String>,
    pub is_active: bool,
    pub two_factor_enabled: bool,
}

#[derive(Debug, Serialize, Deserialize)]
struct ApiError {
    pub error: String,
    pub code: String,
    pub details: Option<HashMap<String, String>>,
}

// Mock user database for web frameworks
struct WebUserDatabase {
    users: Arc<RwLock<HashMap<String, User>>>,
}

impl WebUserDatabase {
    fn new() -> Self {
        let mut users = HashMap::new();
        let user_id = Uuid::new_v4();

        users.insert("admin@startapp.id".to_string(), User {
            id: user_id,
            email: "admin@startapp.id".to_string(),
            password_hash: "$argon2id$v=19$m=19456,t=2,p=1$c29tZXNhbHQ$RdescudvJCsgt3ub+b+dWRWJTmaaJObG".to_string(),
            roles: vec!["admin".to_string()],
            is_active: true,
            is_locked: false,
            two_factor_enabled: false,
            two_factor_methods: vec![],
            account_expires_at: None,
            requires_password_change: false,
        });

        Self {
            users: Arc::new(RwLock::new(users)),
        }
    }
}

#[async_trait::async_trait]
impl UserRepository for WebUserDatabase {
    async fn find_by_email(&self, email: &str) -> Result<Option<User>> {
        let users = self.users.read().await;
        Ok(users.get(email).cloned())
    }

    async fn save(&self, user: &User) -> Result<()> {
        let mut users = self.users.write().await;
        users.insert(user.email.clone(), user.clone());
        Ok(())
    }

    async fn update(&self, user: &User) -> Result<()> {
        let mut users = self.users.write().await;
        users.insert(user.email.clone(), user.clone());
        Ok(())
    }

    async fn delete(&self, user_id: &Uuid) -> Result<()> {
        let mut users = self.users.write().await;
        users.retain(|_, user| user.id != *user_id);
        Ok(())
    }

    async fn find_by_id(&self, user_id: &Uuid) -> Result<Option<User>> {
        let users = self.users.read().await;
        Ok(users.values().find(|user| user.id == *user_id).cloned())
    }
}

// Mock security service for web frameworks
struct WebSecurityService;

#[async_trait::async_trait]
impl SecurityService for WebSecurityService {
    async fn check_rate_limit(&self, _email: &str, _ip_address: Option<&str>) -> Result<()> {
        Ok(())
    }

    async fn analyze_login_attempt(&self, _user_id: &Uuid, _device_info: &DeviceInfo, _ip_address: Option<&str>) -> Result<SecurityFlags> {
        Ok(SecurityFlags {
            new_device: false,
            suspicious_location: false,
            brute_force_detected: false,
            anomaly_detected: false,
        })
    }

    async fn log_failed_auth_attempt(&self, _user_id: &Uuid, _ip_address: Option<&str>) -> Result<()> {
        Ok(())
    }

    async fn log_successful_auth(&self, _user_id: &Uuid, _ip_address: Option<&str>) -> Result<()> {
        Ok(())
    }
}

// JWT middleware state
struct AppState {
    auth_service: AuthService,
    user_database: WebUserDatabase,
    permission_service: PermissionService,
    security_service: WebSecurityService,
}

impl AppState {
    fn new() -> Self {
        Self {
            auth_service: AuthService::with_secret("web_framework_secret_change_in_production"),
            user_database: WebUserDatabase::new(),
            permission_service: PermissionService::new(),
            security_service: WebSecurityService,
        }
    }
}

// Actix Web Integration
#[cfg(feature = "actix")]
mod actix_integration {
    use super::*;
    use actix_web::{web, App, HttpServer, HttpRequest, HttpResponse, Result, middleware};
    use actix_web::dev::ServiceRequest;
    use actix_web_httpauth::middleware::HttpAuthentication;
    use actix_web_httpauth::extractors::bearer::BearerAuth;
    use futures_util::future::{ok, Ready};

    // JWT validation middleware for Actix Web
    async fn jwt_validator(
        req: ServiceRequest,
        credentials: BearerAuth,
    ) -> Result<ServiceRequest, actix_web::Error> {
        let app_state = req.app_data::<web::Data<AppState>>().unwrap();

        match app_state.auth_service.validate_token(credentials.token()).await {
            Ok(validation) if validation.valid => {
                // Add user_id to request extensions
                req.extensions_mut().insert(validation.user_id.unwrap());
                Ok(req)
            }
            _ => Err(actix_web::error::ErrorUnauthorized("Invalid token"))
        }
    }

    // Auth middleware
    async fn auth_middleware(
        req: ServiceRequest,
        credentials: BearerAuth,
    ) -> Result<ServiceRequest, actix_web::Error> {
        jwt_validator(req, credentials).await
    }

    // Login endpoint
    async fn login(
        app_state: web::Data<AppState>,
        login_data: web::Json<LoginRequest>,
        req: HttpRequest,
    ) -> Result<HttpResponse> {
        let client_ip = req.connection_info()
            .realip_remote_addr()
            .or_else(|| req.connection_info().peer_addr())
            .map(|addr| addr.to_string());

        let auth_request = AuthRequest {
            email: login_data.email_or_username.clone(),
            password: login_data.password.clone(),
            ip_address: client_ip,
            device_info: DeviceInfo {
                device_id: "web_client".to_string(),
                user_agent: req.headers()
                    .get("user-agent")
                    .and_then(|h| h.to_str().ok())
                    .unwrap_or("unknown")
                    .to_string(),
                ip_address: client_ip,
                fingerprint: None,
            },
            remember_me: login_data.remember_me,
        };

        match app_state.auth_service.authenticate_enhanced(
            auth_request,
            &app_state.user_database,
            &app_state.security_service
        ).await {
            Ok(result) => {
                let response = AuthResponse {
                    success: true,
                    user_id: Some(result.user_id.to_string()),
                    token: result.token,
                    refresh_token: result.refresh_token,
                    expires_at: Some(result.expires_at.to_rfc3339()),
                    requires_2fa: result.requires_2fa,
                    message: "Authentication successful".to_string(),
                };
                Ok(HttpResponse::Ok().json(response))
            }
            Err(e) => {
                let error = ApiError {
                    error: "Authentication failed".to_string(),
                    code: "AUTH_FAILED".to_string(),
                    details: Some(HashMap::from([("error".to_string(), e.to_string())])),
                };
                Ok(HttpResponse::Unauthorized().json(error))
            }
        }
    }

    // Protected endpoint example
    async fn protected_resource(
        req: HttpRequest,
    ) -> Result<HttpResponse> {
        // Extract user_id from request extensions
        if let Some(user_id) = req.extensions().get::<Uuid>() {
            Ok(HttpResponse::Ok().json(serde_json::json!({
                "message": "Access granted to protected resource",
                "user_id": user_id,
                "timestamp": chrono::Utc::now().to_rfc3339()
            })))
        } else {
            Ok(HttpResponse::Unauthorized().json(serde_json::json!({
                "error": "Unauthorized"
            })))
        }
    }

    // User profile endpoint
    async fn user_profile(
        req: HttpRequest,
        app_state: web::Data<AppState>,
    ) -> Result<HttpResponse> {
        if let Some(user_id) = req.extensions().get::<Uuid>() {
            match app_state.user_database.find_by_id(user_id).await {
                Ok(Some(user)) => {
                    let response = UserResponse {
                        id: user.id.to_string(),
                        email: user.email,
                        roles: user.roles,
                        is_active: user.is_active,
                        two_factor_enabled: user.two_factor_enabled,
                    };
                    Ok(HttpResponse::Ok().json(response))
                }
                _ => Ok(HttpResponse::NotFound().json(serde_json::json!({
                    "error": "User not found"
                })))
            }
        } else {
            Ok(HttpResponse::Unauthorized().json(serde_json::json!({
                "error": "Unauthorized"
            })))
        }
    }

    pub async fn run_actix_server() -> Result<()> {
        println!("🚀 Starting Actix Web server on http://localhost:8080");

        let app_state = web::Data::new(AppState::new());

        HttpServer::new(move || {
            App::new()
                .app_data(app_state.clone())
                .wrap(middleware::Logger::default())
                .wrap(middleware::Compress::default())
                .service(
                    web::scope("/api/v1/auth")
                        .route("/login", web::post().to(login))
                        .route("/protected", web::get().to(protected_resource))
                        .route("/profile", web::get().to(user_profile))
                        .wrap(HttpAuthentication::bearer(auth_middleware))
                )
                .route("/health", web::get().to(|| async {
                    HttpResponse::Ok().json(serde_json::json!({
                        "status": "healthy",
                        "service": "backbone-auth-actix"
                    }))
                }))
        })
        .bind("127.0.0.1:8080")?
        .run()
        .await
        .map_err(|e| anyhow::anyhow!("Actix server error: {}", e))
    }
}

// Axum Integration
#[cfg(feature = "axum")]
mod axum_integration {
    use super::*;
    use axum::{
        Router,
        routing::{get, post},
        extract::{Request, State},
        middleware::{self, Next},
        response::{Json, Response},
        http::{header, StatusCode},
    };
    use tower_http::cors::CorsLayer;
    use tower::ServiceBuilder;

    // JWT validation middleware for Axum
    async fn jwt_middleware(
        State(app_state): State<Arc<AppState>>,
        mut request: Request,
        next: Next,
    ) -> Result<Response, StatusCode> {
        let auth_header = request.headers().get(header::AUTHORIZATION);

        if let Some(auth_header) = auth_header {
            if let Ok(auth_str) = auth_header.to_str() {
                if auth_str.starts_with("Bearer ") {
                    let token = &auth_str[7..];

                    match app_state.auth_service.validate_token(token).await {
                        Ok(validation) if validation.valid => {
                            request.extensions_mut().insert(validation.user_id.unwrap());
                            return Ok(next.run(request).await);
                        }
                        _ => return Err(StatusCode::UNAUTHORIZED),
                    }
                }
            }
        }

        Err(StatusCode::UNAUTHORIZED)
    }

    // Login handler for Axum
    async fn axum_login(
        State(app_state): State<Arc<AppState>>,
        Json(login_data): Json<LoginRequest>,
    ) -> Result<Json<AuthResponse>, StatusCode> {
        let auth_request = AuthRequest {
            email: login_data.email_or_username,
            password: login_data.password,
            ip_address: Some("127.0.0.1".to_string()),
            device_info: DeviceInfo {
                device_id: "axum_client".to_string(),
                user_agent: "Axum-Client/1.0".to_string(),
                ip_address: Some("127.0.0.1".to_string()),
                fingerprint: None,
            },
            remember_me: login_data.remember_me,
        };

        match app_state.auth_service.authenticate_enhanced(
            auth_request,
            &app_state.user_database,
            &app_state.security_service
        ).await {
            Ok(result) => {
                let response = AuthResponse {
                    success: true,
                    user_id: Some(result.user_id.to_string()),
                    token: result.token,
                    refresh_token: result.refresh_token,
                    expires_at: Some(result.expires_at.to_rfc3339()),
                    requires_2fa: result.requires_2fa,
                    message: "Authentication successful".to_string(),
                };
                Ok(Json(response))
            }
            Err(e) => {
                tracing::error!("Authentication failed: {}", e);
                Err(StatusCode::UNAUTHORIZED)
            }
        }
    }

    // Protected endpoint for Axum
    async fn axum_protected(
        request: Request,
    ) -> Result<Json<serde_json::Value>, StatusCode> {
        if let Some(user_id) = request.extensions().get::<Uuid>() {
            Ok(Json(serde_json::json!({
                "message": "Access granted to protected resource",
                "user_id": user_id,
                "framework": "Axum",
                "timestamp": chrono::Utc::now().to_rfc3339()
            })))
        } else {
            Err(StatusCode::UNAUTHORIZED)
        }
    }

    pub async fn run_axum_server() -> Result<()> {
        println!("🚀 Starting Axum server on http://localhost:3000");

        let app_state = Arc::new(AppState::new());

        let app = Router::new()
            .route("/api/v1/auth/login", post(axum_login))
            .route("/api/v1/auth/protected", get(axum_protected))
            .layer(middleware::from_fn_with_state(
                app_state.clone(),
                jwt_middleware
            ))
            .layer(
                ServiceBuilder::new()
                    .layer(CorsLayer::permissive())
            )
            .with_state(app_state)
            .route("/health", get(|| async {
                Json(serde_json::json!({
                    "status": "healthy",
                    "service": "backbone-auth-axum"
                }))
            }));

        let listener = tokio::net::TcpListener::bind("127.0.0.1:3000").await?;
        axum::serve(listener, app).await
            .map_err(|e| anyhow::anyhow!("Axum server error: {}", e))
    }
}

// Warp Integration
#[cfg(feature = "warp")]
mod warp_integration {
    use super::*;
    use warp::{Filter, Reply, Rejection};
    use std::convert::Infallible;

    // JWT validation filter for Warp
    fn with_auth(
        app_state: Arc<AppState>,
    ) -> impl Filter<Extract = (Uuid,), Error = Rejection> + Clone {
        warp::header::<String>("authorization")
            .and(warp::any().map(move || app_state.clone()))
            .and_then(|auth_header: String, app_state: Arc<AppState>| async move {
                if auth_header.starts_with("Bearer ") {
                    let token = &auth_header[7..];

                    match app_state.auth_service.validate_token(token).await {
                        Ok(validation) if validation.valid => {
                            Ok(validation.user_id.unwrap())
                        }
                        _ => Err(warp::reject::custom(AuthError::InvalidToken)),
                    }
                } else {
                    Err(warp::reject::custom(AuthError::InvalidToken))
                }
            })
    }

    // Custom rejection types
    #[derive(Debug)]
    enum AuthError {
        InvalidToken,
    }

    impl warp::reject::Reject for AuthError {}

    // Login handler for Warp
    async fn warp_login(
        login_data: LoginRequest,
        app_state: Arc<AppState>,
    ) -> Result<impl Reply, Rejection> {
        let auth_request = AuthRequest {
            email: login_data.email_or_username,
            password: login_data.password,
            ip_address: Some("127.0.0.1".to_string()),
            device_info: DeviceInfo {
                device_id: "warp_client".to_string(),
                user_agent: "Warp-Client/1.0".to_string(),
                ip_address: Some("127.0.0.1".to_string()),
                fingerprint: None,
            },
            remember_me: login_data.remember_me,
        };

        match app_state.auth_service.authenticate_enhanced(
            auth_request,
            &app_state.user_database,
            &app_state.security_service
        ).await {
            Ok(result) => {
                let response = AuthResponse {
                    success: true,
                    user_id: Some(result.user_id.to_string()),
                    token: result.token,
                    refresh_token: result.refresh_token,
                    expires_at: Some(result.expires_at.to_rfc3339()),
                    requires_2fa: result.requires_2fa,
                    message: "Authentication successful".to_string(),
                };
                Ok(warp::reply::json(&response))
            }
            Err(e) => {
                tracing::error!("Authentication failed: {}", e);
                Ok(warp::reply::json(&serde_json::json!({
                    "error": "Authentication failed"
                })))
            }
        }
    }

    // Protected endpoint for Warp
    async fn warp_protected(user_id: Uuid) -> Result<impl Reply, Rejection> {
        Ok(warp::reply::json(&serde_json::json!({
            "message": "Access granted to protected resource",
            "user_id": user_id,
            "framework": "Warp",
            "timestamp": chrono::Utc::now().to_rfc3339()
        })))
    }

    pub async fn run_warp_server() -> Result<()> {
        println!("🚀 Starting Warp server on http://localhost:3030");

        let app_state = Arc::new(AppState::new());

        // CORS filter
        let cors = warp::cors()
            .allow_any_origin()
            .allow_headers(vec!["authorization", "content-type"])
            .allow_methods(vec!["GET", "POST", "PUT", "DELETE"]);

        // Routes
        let login_route = warp::path!("api" / "v1" / "auth" / "login")
            .and(warp::post())
            .and(warp::body::json())
            .and(warp::any().map(move || app_state.clone()))
            .and_then(warp_login);

        let protected_route = warp::path!("api" / "v1" / "auth" / "protected")
            .and(warp::get())
            .and(with_auth(app_state.clone()))
            .and_then(warp_protected);

        let health_route = warp::path!("health")
            .and(warp::get())
            .map(|| {
                warp::reply::json(&serde_json::json!({
                    "status": "healthy",
                    "service": "backbone-auth-warp"
                }))
            });

        let routes = login_route
            .or(protected_route)
            .or(health_route)
            .with(cors)
            .with(warp::log("backbone_auth"));

        warp::serve(routes)
            .run(([127, 0, 0, 1], 3030))
            .await;

        Ok(())
    }
}

// HTTP Client examples for testing APIs
mod api_client_examples {
    use super::*;
    use reqwest::Client;

    pub struct ApiClient {
        client: Client,
        base_url: String,
        token: Option<String>,
    }

    impl ApiClient {
        pub fn new(base_url: &str) -> Self {
            Self {
                client: Client::new(),
                base_url: base_url.to_string(),
                token: None,
            }
        }

        pub async fn login(&mut self, email: &str, password: &str) -> Result<()> {
            let login_data = LoginRequest {
                email_or_username: email.to_string(),
                password: password.to_string(),
                remember_me: Some(true),
            };

            let response = self.client
                .post(&format!("{}/api/v1/auth/login", self.base_url))
                .json(&login_data)
                .send()
                .await
                .map_err(|e| anyhow::anyhow!("Login request failed: {}", e))?;

            if response.status().is_success() {
                let auth_response: AuthResponse = response.json().await
                    .map_err(|e| anyhow::anyhow!("Failed to parse login response: {}", e))?;

                if auth_response.success {
                    self.token = auth_response.token;
                    println!("✅ Login successful!");
                    println!("🔑 Token received: {}", self.token.as_ref().unwrap().len());
                    return Ok(());
                }
            }

            Err(anyhow::anyhow!("Login failed"))
        }

        pub async fn access_protected_resource(&self) -> Result<()> {
            if let Some(token) = &self.token {
                let response = self.client
                    .get(&format!("{}/api/v1/auth/protected", self.base_url))
                    .header("Authorization", format!("Bearer {}", token))
                    .send()
                    .await
                    .map_err(|e| anyhow::anyhow!("Protected request failed: {}", e))?;

                if response.status().is_success() {
                    let data: serde_json::Value = response.json().await
                        .map_err(|e| anyhow::anyhow!("Failed to parse protected response: {}", e))?;

                    println!("✅ Protected resource accessed!");
                    println!("📄 Response: {}", serde_json::to_string_pretty(&data)?);
                    return Ok(());
                }
            }

            Err(anyhow::anyhow!("Failed to access protected resource"))
        }

        pub async fn test_api_flow(&mut self) -> Result<()> {
            println!("🧪 Testing API Authentication Flow");
            println!("====================================");

            // Test login
            println!("1. Testing login...");
            self.login("admin@startapp.id", "SecureAdminPass123").await?;

            // Test protected resource access
            println!("\n2. Testing protected resource access...");
            self.access_protected_resource().await?;

            println!("\n✅ API authentication flow test completed successfully!");
            Ok(())
        }
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("=== Backbone Auth API Integration Examples ===\n");

    // 1. Framework Comparison
    println!("📊 Web Framework Comparison");
    println!("===========================");

    println!("🚀 Actix Web:");
    println!("✅ Mature and battle-tested");
    println!("✅ Extensive middleware ecosystem");
    println!("✅ Built-in JWT authentication support");
    println!("✅ High performance");
    println!("✅ Type-safe extractors");
    println!();

    println!("🦀 Axum:");
    println!("✅ Built by Tokio team");
    println!("✅ Minimal and ergonomic");
    println!("✅ Tower middleware ecosystem");
    println!("✅ Excellent async performance");
    println!("✅ Growing community");
    println!();

    println!("⚡ Warp:");
    println!("✅ Functional and composable");
    println!("✅ Excellent performance");
    println!("✅ Powerful filter system");
    println!("✅ Zero-cost abstractions");
    println!("✅ Built-in logging support");
    println!();

    // 2. API Testing Examples
    println!("🧪 API Client Testing Examples");
    println!("==============================");

    let mut actix_client = api_client_examples::ApiClient::new("http://localhost:8080");
    let mut axum_client = api_client_examples::ApiClient::new("http://localhost:3000");
    let mut warp_client = api_client_examples::ApiClient::new("http://localhost:3030");

    // Note: These would work if the servers were running
    println!("📝 To test the APIs manually:");
    println!();
    println!("1. Start a server (choose one):");
    println!("   cargo run --example api_integration --features actix");
    println!("   cargo run --example api_integration --features axum");
    println!("   cargo run --example api_integration --features warp");
    println!();

    println!("2. Test with curl:");
    println!("```bash");
    println!("# Login");
    println!("curl -X POST http://localhost:8080/api/v1/auth/login \\");
    println!("  -H \"Content-Type: application/json\" \\");
    println!("  -d '{");
    println!("    \"email_or_username\": \"admin@startapp.id\",");
    println!("    \"password\": \"SecureAdminPass123\",");
    println!("    \"remember_me\": true");
    println!("  }'");
    println!();
    println!("# Access protected resource (replace TOKEN)");
    println!("curl -X GET http://localhost:8080/api/v1/auth/protected \\");
    println!("  -H \"Authorization: Bearer TOKEN\"");
    println!("```");
    println!();

    // 3. Integration Patterns
    println!("🔗 Integration Patterns");
    println!("=========================");

    println!("🛡️ JWT Middleware Pattern:");
    println!("   • Validate tokens on protected routes");
    println!("   • Extract user_id and add to request context");
    println!("   • Handle token expiration gracefully");
    println!("   • Support token refresh mechanisms");
    println!();

    println!("🔐 Authentication Flow:");
    println!("   1. Client sends credentials to /auth/login");
    println!("   2. Server validates credentials and issues JWT");
    println!("   3. Client includes JWT in Authorization header");
    println!("   4. Middleware validates JWT on each request");
    println!("   5. Protected endpoints access user context");
    println!();

    println!("📝 API Design Best Practices:");
    println!("   • Use standard HTTP status codes");
    println!("   • Return consistent error format");
    println!("   • Include request IDs for tracing");
    println!("   • Implement rate limiting");
    println!("   • Use CORS for cross-origin requests");
    println!("   • Provide health check endpoints");
    println!();

    // 4. Production Deployment Considerations
    println!("🚀 Production Deployment");
    println!("========================");

    println!("🔒 Security:");
    println!("• Use HTTPS in production");
    println!("• Set secure cookie flags");
    println!("• Implement CSRF protection");
    println!("• Use environment variables for secrets");
    println!("• Enable security headers");
    println!();

    println!("📈 Performance:");
    println!("• Enable request/response compression");
    println!("• Use connection pooling");
    println!("• Implement caching strategies");
    println!("• Monitor response times");
    println!("• Set up horizontal scaling");
    println!();

    println!("📊 Monitoring:");
    println!("• Add structured logging");
    println!("• Implement health checks");
    println!("• Monitor authentication success/failure rates");
    println!("• Track JWT token lifecycle");
    println!("• Set up alerts for security events");
    println!();

    // 5. Configuration Examples
    println!("⚙️ Configuration Examples");
    println!("=========================");

    println!("📋 Cargo.toml Features:");
    println!("```toml");
    println!("[features]");
    println!("default = []");
    println!("actix = [\"actix-web\", \"actix-web-httpauth\"]");
    println!("axum = [\"axum\", \"tower\", \"tower-http\"]");
    println!("warp = [\"warp\"]");
    println!();
    println!("[dependencies]");
    println!("# Core dependencies");
    println!("backbone-auth = { path = \"..\" }");
    println!("tokio = { version = \"1.0\", features = [\"full\"] }");
    println!("serde = { version = \"1.0\", features = [\"derive\"] }");
    println!("uuid = { version = \"1.0\", features = [\"v4\", \"serde\"] }");
    println!("chrono = { version = \"0.4\", features = [\"serde\"] }");
    println!("anyhow = \"1.0\"");
    println!();
    println!("# Web framework dependencies (choose one)");
    println!("actix-web = { version = \"4.0\", optional = true }");
    println!("actix-web-httpauth = { version = \"0.8\", optional = true }");
    println!("axum = { version = \"0.7\", optional = true }");
    println!("tower = { version = \"0.4\", optional = true }");
    println!("tower-http = { version = \"0.5\", features = [\"cors\"], optional = true }");
    println!("warp = { version = \"0.3\", optional = true }");
    println!();
    println!("# HTTP client for testing");
    println!("reqwest = { version = \"0.11\", features = [\"json\"] }");
    println!("```");
    println!();

    println!("🌍 Environment Variables:");
    println!("```env");
    println!("# Server configuration");
    println!("HOST=0.0.0.0");
    println!("PORT=8080");
    println!("LOG_LEVEL=info");
    println!();
    println!("# Authentication");
    println!("JWT_SECRET=your_very_secure_secret_key_here");
    println!("TOKEN_EXPIRY_HOURS=24");
    println!();
    println!("# CORS");
    println!("CORS_ORIGINS=http://localhost:3000,http://localhost:8080");
    println!("```");
    println!();

    println!("=== API Integration Examples Complete ===");
    println!("🎉 All web framework integration patterns demonstrated!");

    println!("\n📚 Next Steps:");
    println!("1. Choose your preferred web framework");
    println!("2. Run the appropriate example server");
    println!("3. Test with the provided curl commands");
    println!("4. Customize for your application needs");
    println!("5. Add additional middleware and validation");
    println!("6. Set up production monitoring and logging");

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_login_request_serialization() -> Result<()> {
        let login_request = LoginRequest {
            email_or_username: "admin@startapp.id".to_string(),
            password: "SecureAdminPass123".to_string(),
            remember_me: Some(true),
        };

        let json = serde_json::to_string(&login_request)?;
        let parsed: LoginRequest = serde_json::from_str(&json)?;

        assert_eq!(login_request.email_or_username, parsed.email_or_username);
        assert_eq!(login_request.password, parsed.password);
        assert_eq!(login_request.remember_me, parsed.remember_me);

        Ok(())
    }

    #[test]
    fn test_auth_response_serialization() -> Result<()> {
        let auth_response = AuthResponse {
            success: true,
            user_id: Some(Uuid::new_v4().to_string()),
            token: Some("sample_token".to_string()),
            refresh_token: Some("refresh_token".to_string()),
            expires_at: Some(chrono::Utc::now().to_rfc3339()),
            requires_2fa: false,
            message: "Authentication successful".to_string(),
        };

        let json = serde_json::to_string(&auth_response)?;
        let parsed: AuthResponse = serde_json::from_str(&json)?;

        assert_eq!(auth_response.success, parsed.success);
        assert_eq!(auth_response.user_id, parsed.user_id);
        assert_eq!(auth_response.token, parsed.token);
        assert_eq!(auth_response.requires_2fa, parsed.requires_2fa);

        Ok(())
    }

    #[test]
    fn test_api_error_serialization() -> Result<()> {
        let api_error = ApiError {
            error: "Authentication failed".to_string(),
            code: "AUTH_FAILED".to_string(),
            details: Some(HashMap::from([
                ("attempt".to_string(), "1".to_string()),
                ("reason".to_string(), "invalid_credentials".to_string())
            ])),
        };

        let json = serde_json::to_string(&api_error)?;
        let parsed: ApiError = serde_json::from_str(&json)?;

        assert_eq!(api_error.error, parsed.error);
        assert_eq!(api_error.code, parsed.code);
        assert!(parsed.details.is_some());

        Ok(())
    }

    #[tokio::test]
    async fn test_web_user_database_operations() -> Result<()> {
        let user_db = WebUserDatabase::new();
        let user_id = Uuid::new_v4();

        let user = User {
            id: user_id,
            email: "test@example.com".to_string(),
            password_hash: "test_hash".to_string(),
            roles: vec!["user".to_string()],
            is_active: true,
            is_locked: false,
            two_factor_enabled: false,
            two_factor_methods: vec![],
            account_expires_at: None,
            requires_password_change: false,
        };

        // Test save
        user_db.save(&user).await?;

        // Test find by email
        let found_user = user_db.find_by_email("test@example.com").await?;
        assert!(found_user.is_some());
        assert_eq!(found_user.unwrap().email, "test@example.com");

        // Test find by ID
        let found_user = user_db.find_by_id(&user_id).await?;
        assert!(found_user.is_some());
        assert_eq!(found_user.unwrap().id, user_id);

        // Test update
        user_db.update(&user).await?;

        // Test delete
        user_db.delete(&user_id).await?;
        let found_user = user_db.find_by_id(&user_id).await?;
        assert!(found_user.is_none());

        Ok(())
    }
}