// OpenClaw Agent HTTP Client
// Handles agent lifecycle (create/delete/update/get) with resilient HTTP client

use std::pin::Pin;
use std::time::Duration;

use reqwest::Client;
use serde::{Deserialize, Serialize};
use thiserror::Error;

/// Errors from OpenClaw Agent operations
#[derive(Debug, Error)]
pub enum OpenClawError {
    #[error("OpenClaw API request failed: {0}")]
    RequestFailed(String),
    #[error("OpenClaw API returned error: {0}")]
    ApiError(String),
    #[error("OpenClaw API timeout")]
    Timeout,
    #[error("OpenClaw unavailable: {0}")]
    Unavailable(String),
    #[error("agent not found: {0}")]
    NotFound(String),
}

/// OpenClaw Agent configuration passed when creating an agent
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpenClawAgentConfig {
    pub workspace_id: String,
    pub name: String,
}

impl OpenClawAgentConfig {
    pub fn to_json(&self) -> Option<String> {
        serde_json::to_string(self).ok()
    }
}

/// OpenClaw Agent info returned on creation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpenClawAgent {
    pub id: String,
    pub name: String,
    pub status: String,
    pub created_at: Option<String>,
}

/// OpenClaw API response wrapper
#[derive(Debug, Deserialize)]
struct ApiResponse<T> {
    data: Option<T>,
    error: Option<String>,
}

/// Trait for OpenClaw Agent operations — enables testing with mock
pub trait OpenClawAgentClient: Send + Sync {
    /// Create a new agent for the given workspace
    fn create_agent(
        &self,
        config: &OpenClawAgentConfig,
    ) -> Pin<Box<dyn std::future::Future<Output = Result<String, OpenClawError>> + Send + '_>>;

    /// Delete an agent by ID
    fn delete_agent(
        &self,
        agent_id: &str,
    ) -> Pin<Box<dyn std::future::Future<Output = Result<(), OpenClawError>> + Send + '_>>;

    /// Get agent info by ID
    fn get_agent(
        &self,
        agent_id: &str,
    ) -> Pin<Box<dyn std::future::Future<Output = Result<OpenClawAgent, OpenClawError>> + Send + '_>>;

    /// Update agent configuration
    fn update_agent(
        &self,
        agent_id: &str,
        config: &str,
    ) -> Pin<Box<dyn std::future::Future<Output = Result<(), OpenClawError>> + Send + '_>>;
}

/// Real OpenClaw Agent HTTP client
pub struct RealOpenClawAgentClient {
    client: Client,
    base_url: String,
}

impl RealOpenClawAgentClient {
    pub fn new(base_url: String) -> Self {
        let client = Client::builder()
            .timeout(Duration::from_secs(10))
            .build()
            .expect("Failed to build HTTP client");

        Self { client, base_url }
    }

    async fn do_request<R: for<'de> Deserialize<'de>>(
        &self,
        method: reqwest::Method,
        path: &str,
        body: Option<String>,
    ) -> Result<R, OpenClawError> {
        let url = format!("{}{}", self.base_url.trim_end_matches('/'), path);

        let mut request = self.client.request(method, &url);
        request = request.header("Content-Type", "application/json");

        if let Some(body) = body {
            request = request.body(body);
        }

        let response = request.send().await.map_err(|e| {
            if e.is_timeout() {
                OpenClawError::Timeout
            } else {
                OpenClawError::Unavailable(e.to_string())
            }
        })?;

        let status = response.status();
        let body = response.text().await.map_err(|e| {
            OpenClawError::Unavailable(e.to_string())
        })?;

        if status.is_success() {
            serde_json::from_str(&body).map_err(|e| {
                OpenClawError::RequestFailed(format!("failed to parse response: {}", e))
            })
        } else if status.as_u16() == 404 {
            Err(OpenClawError::NotFound(path.to_string()))
        } else {
            Err(OpenClawError::ApiError(format!(
                "status={}: {}",
                status, body
            )))
        }
    }
}

impl OpenClawAgentClient for RealOpenClawAgentClient {
    fn create_agent(
        &self,
        config: &OpenClawAgentConfig,
    ) -> Pin<Box<dyn std::future::Future<Output = Result<String, OpenClawError>> + Send + '_>> {
        let body = match serde_json::to_string(config) {
            Ok(b) => b,
            Err(e) => {
                return Box::pin(async move {
                    Err(OpenClawError::RequestFailed(format!(
                        "failed to serialize config: {}",
                        e
                    )))
                });
            }
        };

        let url = format!("{}/api/v1/agents", self.base_url.trim_end_matches('/'));

        Box::pin(async move {
            let client = Client::builder()
                .timeout(Duration::from_secs(10))
                .build()
                .map_err(|e| OpenClawError::Unavailable(e.to_string()))?;

            let response = client
                .post(&url)
                .header("Content-Type", "application/json")
                .body(body)
                .send()
                .await
                .map_err(|e| {
                    if e.is_timeout() {
                        OpenClawError::Timeout
                    } else {
                        OpenClawError::Unavailable(e.to_string())
                    }
                })?;

            let status = response.status();
            let body = response.text().await.map_err(|e| OpenClawError::Unavailable(e.to_string()))?;

            if status.is_success() {
                serde_json::from_str(&body).map_err(|e| {
                    OpenClawError::RequestFailed(format!("failed to parse response: {}", e))
                })
            } else if status.as_u16() == 404 {
                Err(OpenClawError::NotFound(url))
            } else {
                Err(OpenClawError::ApiError(format!("status={}: {}", status, body)))
            }
            .and_then(|r: ApiResponse<OpenClawAgent>| {
                r.data
                    .map(|a| a.id)
                    .ok_or_else(|| OpenClawError::ApiError(r.error.unwrap_or_default()))
            })
        })
    }

    fn delete_agent(
        &self,
        agent_id: &str,
    ) -> Pin<Box<dyn std::future::Future<Output = Result<(), OpenClawError>> + Send + '_>> {
        let url = format!(
            "{}/api/v1/agents/{}",
            self.base_url.trim_end_matches('/'),
            agent_id
        );

        Box::pin(async move {
            let client = Client::builder()
                .timeout(Duration::from_secs(10))
                .build()
                .map_err(|e| OpenClawError::Unavailable(e.to_string()))?;

            let response = client
                .delete(&url)
                .header("Content-Type", "application/json")
                .send()
                .await
                .map_err(|e| {
                    if e.is_timeout() {
                        OpenClawError::Timeout
                    } else {
                        OpenClawError::Unavailable(e.to_string())
                    }
                })?;

            let status = response.status();

            if status.is_success() || status.as_u16() == 404 {
                Ok(())
            } else {
                let body = response.text().await.unwrap_or_default();
                Err(OpenClawError::ApiError(format!("status={}: {}", status, body)))
            }
        })
    }

    fn get_agent(
        &self,
        agent_id: &str,
    ) -> Pin<Box<dyn std::future::Future<Output = Result<OpenClawAgent, OpenClawError>> + Send + '_>> {
        let agent_id = agent_id.to_string();
        let url = format!(
            "{}/api/v1/agents/{}",
            self.base_url.trim_end_matches('/'),
            agent_id
        );

        Box::pin(async move {
            let client = Client::builder()
                .timeout(Duration::from_secs(10))
                .build()
                .map_err(|e| OpenClawError::Unavailable(e.to_string()))?;

            let response = client
                .get(&url)
                .header("Content-Type", "application/json")
                .send()
                .await
                .map_err(|e| {
                    if e.is_timeout() {
                        OpenClawError::Timeout
                    } else {
                        OpenClawError::Unavailable(e.to_string())
                    }
                })?;

            let status = response.status();
            let body = response.text().await.map_err(|e| OpenClawError::Unavailable(e.to_string()))?;

            if status.is_success() {
                serde_json::from_str(&body).map_err(|e| {
                    OpenClawError::RequestFailed(format!("failed to parse response: {}", e))
                })
            } else if status.as_u16() == 404 {
                Err(OpenClawError::NotFound(agent_id.to_string()))
            } else {
                Err(OpenClawError::ApiError(format!("status={}: {}", status, body)))
            }
            .and_then(|r: ApiResponse<OpenClawAgent>| {
                r.data
                    .ok_or_else(|| OpenClawError::ApiError(r.error.unwrap_or_else(|| "not found".into())))
            })
        })
    }

    fn update_agent(
        &self,
        agent_id: &str,
        config: &str,
    ) -> Pin<Box<dyn std::future::Future<Output = Result<(), OpenClawError>> + Send + '_>> {
        let url = format!(
            "{}/api/v1/agents/{}",
            self.base_url.trim_end_matches('/'),
            agent_id
        );
        let config = config.to_string();

        Box::pin(async move {
            let client = Client::builder()
                .timeout(Duration::from_secs(10))
                .build()
                .map_err(|e| OpenClawError::Unavailable(e.to_string()))?;

            let response = client
                .put(&url)
                .header("Content-Type", "application/json")
                .body(config)
                .send()
                .await
                .map_err(|e| {
                    if e.is_timeout() {
                        OpenClawError::Timeout
                    } else {
                        OpenClawError::Unavailable(e.to_string())
                    }
                })?;

            let status = response.status();

            if status.is_success() || status.as_u16() == 404 {
                Ok(())
            } else {
                let body = response.text().await.unwrap_or_default();
                Err(OpenClawError::ApiError(format!("status={}: {}", status, body)))
            }
        })
    }
}

/// Mock OpenClaw Agent client for testing
pub struct MockOpenClawAgentClient {
    pub agents: std::sync::Mutex<std::collections::HashMap<String, OpenClawAgent>>,
}

impl MockOpenClawAgentClient {
    pub fn new() -> Self {
        Self {
            agents: std::sync::Mutex::new(std::collections::HashMap::new()),
        }
    }
}

impl Default for MockOpenClawAgentClient {
    fn default() -> Self {
        Self::new()
    }
}

impl OpenClawAgentClient for MockOpenClawAgentClient {
    fn create_agent(
        &self,
        config: &OpenClawAgentConfig,
    ) -> Pin<Box<dyn std::future::Future<Output = Result<String, OpenClawError>> + Send + '_>> {
        let agent_id = format!("agent-{}", uuid::Uuid::new_v4());
        let agent = OpenClawAgent {
            id: agent_id.clone(),
            name: config.name.clone(),
            status: "active".to_string(),
            created_at: Some(chrono::Utc::now().to_rfc3339()),
        };
        self.agents.lock().unwrap().insert(agent_id.clone(), agent);

        Box::pin(async move { Ok(agent_id) })
    }

    fn delete_agent(
        &self,
        agent_id: &str,
    ) -> Pin<Box<dyn std::future::Future<Output = Result<(), OpenClawError>> + Send + '_>> {
        let result = self
            .agents
            .lock()
            .unwrap()
            .remove(agent_id)
            .ok_or(OpenClawError::NotFound(agent_id.to_string()));

        Box::pin(async move { result.map(|_| ()) })
    }

    fn get_agent(
        &self,
        agent_id: &str,
    ) -> Pin<Box<dyn std::future::Future<Output = Result<OpenClawAgent, OpenClawError>> + Send + '_>> {
        let result = self
            .agents
            .lock()
            .unwrap()
            .get(agent_id)
            .cloned()
            .ok_or(OpenClawError::NotFound(agent_id.to_string()));

        Box::pin(async move { result })
    }

    fn update_agent(
        &self,
        agent_id: &str,
        _config: &str,
    ) -> Pin<Box<dyn std::future::Future<Output = Result<(), OpenClawError>> + Send + '_>> {
        let exists = self.agents.lock().unwrap().contains_key(agent_id);
        let result = if exists {
            Ok(())
        } else {
            Err(OpenClawError::NotFound(agent_id.to_string()))
        };

        Box::pin(async move { result })
    }
}
