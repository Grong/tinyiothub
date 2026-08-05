// Thing module error types

use axum::{
    Json,
    http::StatusCode,
    response::{IntoResponse, Response},
};
use serde_json::json;

#[derive(Debug, thiserror::Error)]
pub enum ThingError {
    #[error("Thing not found: {0}")]
    NotFound(String),

    #[error("Thing name already exists in this workspace: {0}")]
    NameConflict(String),

    #[error("Parent cycle detected: {thing_id} → {parent_id} would create a loop")]
    CycleDetected { thing_id: String, parent_id: String },

    #[error("Cannot delete thing with {0} children")]
    HasChildren(usize),

    #[error("Action not supported: {0}")]
    ActionNotSupported(String),

    #[error("Validation failed: {0}")]
    Validation(String),

    #[error("Database error: {0}")]
    Database(String),

    #[error("Workspace not found: {0}")]
    WorkspaceNotFound(String),
}

/// HTTP status code mapping for each error variant.
impl ThingError {
    pub fn status_code(&self) -> StatusCode {
        match self {
            ThingError::NotFound(_) => StatusCode::NOT_FOUND,
            ThingError::NameConflict(_) => StatusCode::CONFLICT,
            ThingError::CycleDetected { .. } => StatusCode::CONFLICT,
            ThingError::HasChildren(_) => StatusCode::CONFLICT,
            ThingError::ActionNotSupported(_) => StatusCode::BAD_REQUEST,
            ThingError::Validation(_) => StatusCode::UNPROCESSABLE_ENTITY,
            ThingError::Database(_) => StatusCode::INTERNAL_SERVER_ERROR,
            ThingError::WorkspaceNotFound(_) => StatusCode::NOT_FOUND,
        }
    }
}

impl IntoResponse for ThingError {
    fn into_response(self) -> Response {
        let status = self.status_code();
        let body = json!({
            "code": status.as_u16() as i32,
            "message": self.to_string(),
        });
        (status, Json(body)).into_response()
    }
}

impl From<sqlx::Error> for ThingError {
    fn from(e: sqlx::Error) -> Self {
        // Detect UNIQUE constraint violation (SQLite error code 2067)
        if let sqlx::Error::Database(ref db_err) = e
            && db_err.code().as_deref() == Some("2067")
        {
            return ThingError::NameConflict("name already exists in this workspace".to_string());
        }
        ThingError::Database(e.to_string())
    }
}
