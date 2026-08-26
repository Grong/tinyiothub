// Session Index Service — lightweight session lifecycle management
//
// With zeroclaw v0.7.5, chat history is managed by Agent::history() (in-memory).
// This service only maintains the session index for listing/labeling/deleting sessions.

use std::sync::Arc;
use tinyiothub_storage::Db;
use tinyiothub_storage::session::{Session, SessionError};

/// Session index service for managing session lifecycle
pub struct SessionService {
    db: Arc<Db>,
}

impl SessionService {
    pub fn new(db: Arc<Db>) -> Self {
        Self { db }
    }

    pub async fn get_session(&self, session_key: &str) -> Result<Option<Session>, SessionError> {
        self.db.get_session(session_key).await
    }

    pub async fn create_session(
        &self,
        session_key: String,
        workspace_id: String,
        agent_id: String,
    ) -> Result<Session, SessionError> {
        let session = Session::new(session_key, workspace_id, agent_id);
        self.db.create_session(&session).await?;
        Ok(session)
    }

    pub async fn update_label(&self, session_key: &str, label: impl Into<String>) -> Result<Session, SessionError> {
        let mut session = self
            .db
            .get_session(session_key)
            .await?
            .ok_or_else(|| SessionError::NotFound(session_key.to_string()))?;
        session.set_label(label);
        self.db.update_session(&session).await?;
        Ok(session)
    }

    pub async fn delete_session(&self, session_key: &str) -> Result<(), SessionError> {
        self.db.delete_session(session_key).await
    }

    pub async fn list_sessions(
        &self,
        workspace_id: Option<&str>,
        agent_id: Option<&str>,
        limit: usize,
        offset: usize,
    ) -> Result<Vec<Session>, SessionError> {
        self.db.list_sessions(workspace_id, agent_id, limit, offset).await
    }
}
