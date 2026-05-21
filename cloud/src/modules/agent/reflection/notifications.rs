use std::{collections::HashMap, convert::Infallible, sync::Arc};

use axum::response::{
    Sse,
    sse::{Event, KeepAlive},
};
use futures::stream::Stream;
use tokio::sync::broadcast;
use tokio_stream::{StreamExt, wrappers::BroadcastStream};

/// Per-workspace broadcast channels for SSE skill notifications.
pub struct NotificationService {
    channels: Arc<tokio::sync::RwLock<HashMap<String, broadcast::Sender<String>>>>,
}

impl Default for NotificationService {
    fn default() -> Self {
        Self { channels: Arc::new(tokio::sync::RwLock::new(HashMap::new())) }
    }
}

impl NotificationService {
    pub fn new() -> Self {
        Self::default()
    }

    pub async fn broadcast(&self, workspace_id: &str, event_type: &str, message: &str) {
        let msg = serde_json::json!({
            "type": event_type,
            "message": message,
            "timestamp": chrono::Utc::now().to_rfc3339(),
        })
        .to_string();

        let channels = self.channels.read().await;
        if let Some(tx) = channels.get(workspace_id) {
            let _ = tx.send(msg);
        }
    }

    pub async fn subscribe(&self, workspace_id: &str) -> broadcast::Receiver<String> {
        let mut channels = self.channels.write().await;
        let tx =
            channels.entry(workspace_id.to_string()).or_insert_with(|| broadcast::channel(64).0);
        tx.subscribe()
    }

    /// Create an SSE stream for a workspace.
    pub async fn sse_stream(
        self: Arc<Self>,
        workspace_id: String,
    ) -> impl Stream<Item = Result<Event, Infallible>> {
        let rx = self.subscribe(&workspace_id).await;
        BroadcastStream::new(rx).map(|result| match result {
            Ok(msg) => Ok(Event::default().data(msg).event("skill_notification")),
            Err(_) => Ok(Event::default().comment("stream lagged")),
        })
    }

    pub async fn notify_skill_discovered(
        &self,
        workspace_id: &str,
        skill_name: &str,
        skill_description: &str,
    ) {
        let message = format!("我发现你经常「{}」，要不要我把它自动化？", skill_description);
        self.broadcast(workspace_id, "skill_discovered", &message).await;
        tracing::info!(
            workspace_id,
            skill_name,
            skill_description,
            "Skill discovery notification sent"
        );
    }
}

/// Generate a weekly digest via LLM based on recent memories.
pub async fn generate_weekly_digest(
    memory_store: &dyn tinyiothub_core::memory::MemoryStore,
    workspace_id: &str,
    agent_id: &str,
) -> anyhow::Result<String> {
    let since =
        (chrono::Utc::now() - chrono::Duration::days(7)).format("%Y-%m-%dT%H:%M:%S").to_string();
    let new_memories = memory_store.get_since(workspace_id, agent_id, &since).await?;

    let prompt = format!(
        "Generate a brief weekly summary (~100 words) of what you learned:\n\
         New facts: {} items\n\
         Write in the user's preferred language, friendly tone.\n\n\
         Recent memories:\n{}",
        new_memories.len(),
        new_memories.iter().map(|m| format!("- {}", m.content)).collect::<Vec<_>>().join("\n"),
    );

    tracing::info!(
        workspace_id,
        agent_id,
        memory_count = new_memories.len(),
        "Weekly digest prompt prepared"
    );
    Ok(prompt)
}

/// SSE handler for agent skill notifications (public — auth via query param).
pub async fn handle_notification_sse(
    axum::extract::State(state): axum::extract::State<crate::shared::app_state::AppState>,
    axum::extract::Query(params): axum::extract::Query<std::collections::HashMap<String, String>>,
) -> Result<Sse<impl Stream<Item = Result<Event, Infallible>>>, axum::http::StatusCode> {
    // Auth via ?token=... query param (EventSource doesn't support custom headers)
    let token = params.get("token").cloned().unwrap_or_default();
    let claims = crate::shared::security::jwt::validate_jwt(&token)
        .map_err(|_| axum::http::StatusCode::UNAUTHORIZED)?;
    let ws = claims.workspace_id;

    let svc = Arc::clone(&state.agent_pool.notification_service);
    let stream = svc.sse_stream(ws).await;
    Ok(Sse::new(stream).keep_alive(KeepAlive::default()))
}
