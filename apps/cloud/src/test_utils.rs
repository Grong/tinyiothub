//! Test utilities for integration tests
//!
//! Provides shared harness for handler-level integration tests:
//! - `setup_test_app()` — creates Router with in-memory SQLite
//! - `create_test_token()` — generates valid JWT for authenticated requests

use std::sync::OnceLock;

use axum::Router;
use http_body_util::BodyExt;
use serde_json::Value;
use tinyiothub_core::config::ApplicationSettings;

use crate::shared::app_state::AppState;

static TEST_SETTINGS: OnceLock<ApplicationSettings> = OnceLock::new();

/// Initialize test configuration (runs once across all tests).
///
/// Sets environment variables for test config, then loads the settings.
/// Uses `OnceLock` to ensure it only runs once, even with parallel tests.
/// (G6: the loaded settings live in this test-harness OnceLock and are
/// passed into `AppState::new` — no process-global config accessor.)
fn ensure_test_config() -> &'static ApplicationSettings {
    TEST_SETTINGS.get_or_init(|| {
        // SAFETY: set_var is called once during test initialization, before any threads read env vars.
        // This is safe because tests run sequentially per process and config is initialized once via OnceCell.
        unsafe {
            std::env::set_var("TINYIOTHUB__SERVER__HOST", "127.0.0.1");
            std::env::set_var("TINYIOTHUB__SERVER__PORT", "19999");
            std::env::set_var("TINYIOTHUB__DATABASE__URL", "sqlite::memory:");
            std::env::set_var(
                "TINYIOTHUB__SECURITY__JWT__SECRET",
                "test-jwt-secret-that-is-at-least-32-chars-long",
            );
            std::env::set_var("TINYIOTHUB__SECURITY__JWT__EXPIRATION_SECS", "3600");
            std::env::set_var("TINYIOTHUB__SECURITY__JWT__ISSUER", "tinyiothub-test");
            std::env::set_var("TINYIOTHUB__SECURITY__JWT__AUDIENCE", "tinyiothub-test");
            std::env::set_var("TINYIOTHUB__MQTT__PRIMARY__HOST", "localhost");
            std::env::set_var("TINYIOTHUB__MQTT__PRIMARY__PORT", "1883");
            std::env::set_var("TINYIOTHUB__MQTT__CLIENT__CLIENT_ID", "test-client");
            std::env::set_var("TINYIOTHUB__MQTT__TOPICS__PREFIX", "test/");
            std::env::set_var("TINYIOTHUB__MQTT__TOPICS__HEARTBEAT", "heartbeat");
            std::env::set_var("TINYIOTHUB__MQTT__TOPICS__DEVICE_REGISTRATION", "register");
            std::env::set_var("TINYIOTHUB__MQTT__TOPICS__COMMAND", "command");
            std::env::set_var("TINYIOTHUB__MQTT__TOPICS__DATA_UPLOAD", "data");
            std::env::set_var("TINYIOTHUB__MQTT__TOPICS__ALARM", "alarm");
            std::env::set_var("TINYIOTHUB__LOGGING__LEVEL", "info");
            std::env::set_var("TINYIOTHUB__ENVIRONMENT__NAME", "test");
            std::env::set_var("TINYIOTHUB__MINIMAX__BASE_URL", "https://test.example.com");
            std::env::set_var("TINYIOTHUB__MINIMAX__AUTH_TOKEN", "test-token");
            std::env::set_var("TINYIOTHUB__MINIMAX__MODEL", "test-model");
            // Use "none" memory backend in tests to avoid SQLite file lock
            // contention when multiple tests open agent memory concurrently.
            std::env::set_var("TINYIOTHUB__AGENT__MEMORY_BACKEND", "none");
        }

        // Load config — panic if it fails so we know immediately
        let settings = crate::shared::config::load_configuration().expect("Failed to load test config");

        // JWT 机制服务（G2 构造注入）。测试进程级 OnceLock 注册一次；
        // resolver 闭包共享同一实例（JwtService 无内部状态，克隆廉价）。
        let jwt_service = std::sync::Arc::new(tinyiothub_authn::jwt::JwtService::new(
            tinyiothub_authn::jwt::JwtSettings {
                secret: settings.security.jwt.secret.clone(),
                harmonyos_enabled: settings.harmonyos.enabled,
            },
        ));

        // Register the tenant resolver (P4-Task15) so tinyiothub_web's
        // WorkspaceScope/AuthClaims extractors validate test JWTs exactly as
        // the binary does (mirrors main.rs). OnceLock makes re-set a no-op.
        let svc = jwt_service.clone();
        tinyiothub_web::middleware::workspace::set_tenant_resolver(Box::new(move |token| {
            svc.validate_jwt(token)
                .ok()
                .map(|c| tinyiothub_web::middleware::workspace::TenantClaims {
                    user_id: c.user_id,
                    tenant_id: c.tenant_id,
                    workspace_id: c.workspace_id,
                })
        }));

        settings
    })
}

/// Create a test application router with in-memory SQLite database.
///
/// Returns a `Router` ready for `oneshot()` testing. Sets up:
/// - In-memory SQLite with all migrations
/// - Test JWT configuration
/// - Full API route tree
pub async fn setup_test_app() -> Router {
    let (app_state, _pool) = setup_test_app_with_pool().await;

    // Create API router (same as production, without MCP/agent init)
    let api_router = crate::api::create_router(&app_state);

    Router::new().nest("/api", api_router).with_state(app_state)
}

/// Create test AppState and return it along with the pool (for seeding test data).
pub async fn setup_test_app_with_pool() -> (AppState, sqlx::SqlitePool) {
    let settings = ensure_test_config();

    let app_state = create_test_app_state(settings).await;
    let pool = app_state.db_pool().clone();
    (app_state, pool)
}

/// Seed a tenant and workspace for testing cross-workspace isolation.
pub async fn seed_test_workspace(pool: &sqlx::SqlitePool, tenant_id: &str, workspace_id: &str) {
    let now = chrono::Utc::now().format("%Y-%m-%d %H:%M:%S").to_string();

    sqlx::query(
        "INSERT OR IGNORE INTO tenants (id, name, slug, status, plan_id, subscription_status, timezone, locale, created_at, updated_at) VALUES (?, ?, ?, 'active', 'plan_free', 'active', 'UTC', 'zh-CN', ?, ?)",
    )
    .bind(tenant_id)
    .bind(format!("Test Tenant {}", tenant_id))
    .bind(tenant_id)
    .bind(&now)
    .bind(&now)
    .execute(pool)
    .await
    .expect("Failed to seed test tenant");

    sqlx::query(
        "INSERT OR IGNORE INTO workspaces (id, name, description, tenant_id, created_at, updated_at) VALUES (?, ?, ?, ?, ?, ?)",
    )
    .bind(workspace_id)
    .bind(format!("Test Workspace {}", workspace_id))
    .bind("Test workspace for cross-workspace isolation tests")
    .bind(tenant_id)
    .bind(&now)
    .bind(&now)
    .execute(pool)
    .await
    .expect("Failed to seed test workspace");
}

/// Create an AppState backed by in-memory SQLite with migrations applied.
///
/// Runs cloud migrations, skipping test-data migrations that reference
/// non-existent devices.
async fn create_test_app_state(settings: &ApplicationSettings) -> AppState {
    use std::sync::Arc;

    use tinyiothub_storage::cache::DeviceCache;

    // In-memory SQLite — no temp file, no cleanup issues
    let pool = sqlx::SqlitePool::connect("sqlite::memory:")
        .await
        .expect("Failed to create in-memory SQLite pool");

    // Run migrations via centralized module (handles skip lists, orphaned
    // records, and schema consistency automatically).
    tinyiothub_storage::migrations::run_migrations(&pool)
        .await
        .expect("Failed to run migrations");

    // Seed a test user so FK constraints (created_by REFERENCES users(id)) don't fail
    sqlx::query(
        "INSERT OR IGNORE INTO users (id, username, password_hash, display_name, is_enabled)
         VALUES (?, ?, ?, ?, ?)",
    )
    .bind("user-1")
    .bind("test-user")
    .bind("$argon2id$v=19$m=19456,t=2,p=1$test")
    .bind("Test User")
    .bind(true)
    .execute(&pool)
    .await
    .expect("Failed to seed test user");

    let device_cache = Arc::new(DeviceCache::new());

    AppState::new(device_cache, pool, settings)
}

fn test_jwt_service() -> tinyiothub_authn::jwt::JwtService {
    let settings = ensure_test_config();
    tinyiothub_authn::jwt::JwtService::new(tinyiothub_authn::jwt::JwtSettings {
        secret: settings.security.jwt.secret.clone(),
        harmonyos_enabled: settings.harmonyos.enabled,
    })
}

/// Generate a valid JWT token for testing authenticated endpoints.
pub fn create_test_token(user_id: &str, tenant_id: &str) -> String {
    test_jwt_service()
        .generate_token(user_id, "test-user", tenant_id, "ws-default-001")
        .expect("Failed to generate test token")
}

/// Generate a JWT token with explicit workspace_id for cross-tenant isolation tests.
pub fn create_test_token_with_workspace(user_id: &str, tenant_id: &str, workspace_id: &str) -> String {
    test_jwt_service()
        .generate_token(user_id, "test-user", tenant_id, workspace_id)
        .expect("Failed to generate test token")
}

/// Build an Authorization header value from a token.
pub fn auth_header(token: &str) -> String {
    format!("Bearer {}", token)
}

/// Extract the response body as JSON `Value`.
pub async fn response_json(response: axum::response::Response) -> Value {
    let body = response.into_body().collect().await.unwrap().to_bytes();
    serde_json::from_slice(&body)
        .unwrap_or_else(|_| panic!("Failed to parse response as JSON: {}", String::from_utf8_lossy(&body)))
}

/// Extract the response status code and body as JSON.
pub async fn response_parts(response: axum::response::Response) -> (axum::http::StatusCode, Value) {
    let status = response.status();
    let body = response_json(response).await;
    (status, body)
}
