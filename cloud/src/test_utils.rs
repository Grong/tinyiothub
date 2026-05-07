//! Test utilities for integration tests
//!
//! Provides shared harness for handler-level integration tests:
//! - `setup_test_app()` — creates Router with in-memory SQLite
//! - `create_test_token()` — generates valid JWT for authenticated requests

use std::sync::OnceLock;

use axum::Router;
use http_body_util::BodyExt;
use serde_json::Value;

use crate::shared::app_state::AppState;

static TEST_CONFIG: OnceLock<()> = OnceLock::new();

/// Initialize test configuration (runs once across all tests).
///
/// Sets environment variables for test config, then initializes the global config.
/// Uses `OnceCell` to ensure it only runs once, even with parallel tests.
fn ensure_test_config() {
    TEST_CONFIG.get_or_init(|| {
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
        }

        // Initialize config — panic if it fails so we know immediately
        crate::shared::config::initialize().expect("Failed to initialize test config");
    });
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
    let api_router = crate::api::create_router();

    Router::new().nest("/api", api_router).with_state(app_state)
}

/// Create test AppState and return it along with the pool (for seeding test data).
pub async fn setup_test_app_with_pool() -> (AppState, sqlx::SqlitePool) {
    ensure_test_config();

    let app_state = create_test_app_state().await;
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
async fn create_test_app_state() -> AppState {
    use std::{path::Path, sync::Arc};

    use tinyiothub_storage::cache::DeviceCache;

    // In-memory SQLite — no temp file, no cleanup issues
    let pool = sqlx::SqlitePool::connect("sqlite::memory:")
        .await
        .expect("Failed to create in-memory SQLite pool");

    // Run migrations via centralized module (handles skip lists, orphaned
    // records, and schema consistency automatically).
    crate::shared::persistence::migrations::run_migrations(&pool)
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

    // Initialize START_TIME for uptime tests
    let _ =
        crate::modules::monitoring::handler::health::START_TIME.set(std::time::SystemTime::now());

    AppState::new(device_cache, pool)
}

/// Generate a valid JWT token for testing authenticated endpoints.
pub fn create_test_token(user_id: &str, tenant_id: &str) -> String {
    crate::shared::security::jwt::generate_token(user_id, "test-user", tenant_id, "ws-default-001")
        .expect("Failed to generate test token")
}

/// Generate a JWT token with explicit workspace_id for cross-tenant isolation tests.
pub fn create_test_token_with_workspace(
    user_id: &str,
    tenant_id: &str,
    workspace_id: &str,
) -> String {
    crate::shared::security::jwt::generate_token(user_id, "test-user", tenant_id, workspace_id)
        .expect("Failed to generate test token")
}

/// Build an Authorization header value from a token.
pub fn auth_header(token: &str) -> String {
    format!("Bearer {}", token)
}

/// Extract the response body as JSON `Value`.
pub async fn response_json(response: axum::response::Response) -> Value {
    let body = response.into_body().collect().await.unwrap().to_bytes();
    serde_json::from_slice(&body).unwrap_or_else(|_| {
        panic!("Failed to parse response as JSON: {}", String::from_utf8_lossy(&body))
    })
}

/// Extract the response status code and body as JSON.
pub async fn response_parts(response: axum::response::Response) -> (axum::http::StatusCode, Value) {
    let status = response.status();
    let body = response_json(response).await;
    (status, body)
}
