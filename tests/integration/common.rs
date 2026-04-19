//! Shared test utilities for integration tests.
//!
//! TODO: This module is designed for a future `integration-tests` crate.
//! Cargo workspaces do not auto-compile `tests/` at the root level;
//! these will be wired up once a dedicated test crate is created.

use std::sync::OnceLock;

/// Test configuration — isolated from production settings.
#[derive(Debug, Clone)]
pub struct TestConfig {
    pub db_url: String,
    pub server_port: u16,
}

impl Default for TestConfig {
    fn default() -> Self {
        Self {
            db_url: ":memory:".into(),
            server_port: 0, // OS-assigned ephemeral port
        }
    }
}

/// Global test configuration (set once per test process).
static TEST_CONFIG: OnceLock<TestConfig> = OnceLock::new();

/// Initialize and return the test configuration.
pub fn test_config() -> &'static TestConfig {
    TEST_CONFIG.get_or_init(TestConfig::default)
}

/// Generate a unique name for test resources to avoid collisions.
pub fn unique_name(prefix: &str) -> String {
    let ts = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_millis();
    format!("{}_{}", prefix, ts)
}

/// Retry an async operation with exponential backoff.
pub async fn retry<T, E, F, Fut>(mut f: F, max_attempts: u32) -> Result<T, E>
where
    F: FnMut() -> Fut,
    Fut: std::future::Future<Output = Result<T, E>>,
{
    let mut delay_ms = 100;
    for attempt in 1..=max_attempts {
        match f().await {
            Ok(v) => return Ok(v),
            Err(e) if attempt == max_attempts => return Err(e),
            Err(_) => {
                tokio::time::sleep(tokio::time::Duration::from_millis(delay_ms)).await;
                delay_ms *= 2;
            }
        }
    }
    unreachable!()
}
