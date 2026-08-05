// Session Index Service — lightweight session lifecycle management
//
// With zeroclaw v0.7.5, chat history is managed by Agent::history() (in-memory).
// This service only maintains the session index for listing/labeling/deleting sessions.

use std::sync::Arc;

use super::types::{Session, SessionError, SessionRepository};

/// Session index service for managing session lifecycle
pub struct SessionService {
    repo: Arc<dyn SessionRepository>,
}

impl SessionService {
    pub fn new(repo: Arc<dyn SessionRepository>) -> Self {
        Self { repo }
    }

    pub async fn get_session(&self, session_key: &str) -> Result<Option<Session>, SessionError> {
        self.repo.get(session_key).await
    }

    pub async fn create_session(
        &self,
        session_key: String,
        workspace_id: String,
        agent_id: String,
    ) -> Result<Session, SessionError> {
        let session = Session::new(session_key, workspace_id, agent_id);
        self.repo.create(&session).await?;
        Ok(session)
    }

    pub async fn update_label(
        &self,
        session_key: &str,
        label: impl Into<String>,
    ) -> Result<Session, SessionError> {
        let mut session = self
            .repo
            .get(session_key)
            .await?
            .ok_or_else(|| SessionError::NotFound(session_key.to_string()))?;
        session.set_label(label);
        self.repo.update(&session).await?;
        Ok(session)
    }

    pub async fn delete_session(&self, session_key: &str) -> Result<(), SessionError> {
        self.repo.delete(session_key).await
    }

    pub async fn list_sessions(
        &self,
        workspace_id: Option<&str>,
        agent_id: Option<&str>,
        limit: usize,
        offset: usize,
    ) -> Result<Vec<Session>, SessionError> {
        self.repo.list(workspace_id, agent_id, limit, offset).await
    }
}
