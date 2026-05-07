// Workspace Files API for Agents
// HTTP endpoint handlers for agent workspace markdown files
//
// Endpoints:
// - GET  /agents/{id}/files
// - GET  /agents/{id}/files/{filename}
// - PUT  /agents/{id}/files/{filename}
//
// Supported files: IDENTITY.md, SOUL.md, AGENTS.md, USER.md, TOOLS.md, MEMORY.md, HEARTBEAT.md, BOOTSTRAP.md

use std::path::PathBuf;

use axum::{
    extract::{Path, State},
    response::Json,
};
use serde::{Deserialize, Serialize};
use tinyiothub_web::response::ApiResponseBuilder;

use crate::{
    api::middleware::WorkspaceScope,
    shared::{api_response::ApiResponse, app_state::AppState, security::jwt::Claims},
};

/// Supported workspace files (pub(crate) for testing)
pub(crate) const WORKSPACE_FILES: &[&str] = &[
    "IDENTITY.md",
    "SOUL.md",
    "AGENTS.md",
    "USER.md",
    "TOOLS.md",
    "MEMORY.md",
    "HEARTBEAT.md",
    "BOOTSTRAP.md",
];

/// Response for list of workspace files
#[derive(Debug, Serialize)]
pub struct WorkspaceFilesListResponse {
    pub files: Vec<WorkspaceFileInfo>,
}

/// Info about a workspace file
#[derive(Debug, Serialize)]
pub struct WorkspaceFileInfo {
    pub name: String,
}

/// Response for workspace file content
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceFileResponse {
    pub name: String,
    pub content: String,
}

/// Request for updating a workspace file
#[derive(Debug, Deserialize)]
pub struct UpdateWorkspaceFileRequest {
    pub content: String,
}

/// Maximum file size: 1MB (pub(crate) for testing)
pub(crate) const MAX_FILE_SIZE: usize = 1024 * 1024;

/// Validate workspace file path
fn validate_workspace_file_path(workspace_id: &str, filename: &str) -> Result<PathBuf, String> {
    // Reject path traversal attempts
    if workspace_id.contains("..") || filename.contains("..") {
        return Err("Invalid path: traversal not allowed".to_string());
    }
    if workspace_id.starts_with('/') || filename.starts_with('/') {
        return Err("Invalid path: absolute paths not allowed".to_string());
    }

    // Validate filename is in our allowlist
    if !WORKSPACE_FILES.contains(&filename) {
        return Err(format!("Invalid filename: {}. Allowed: {:?}", filename, WORKSPACE_FILES));
    }

    let workspace_dir = crate::shared::paths::workspace_dir(workspace_id);

    let file_path = workspace_dir.join(filename);

    // Verify the resolved path is still under workspace_dir (defense in depth)
    let canonical = file_path.canonicalize().map_err(|_| "Invalid path")?;
    let workspace_canonical =
        workspace_dir.canonicalize().map_err(|_| "Invalid workspace directory")?;

    if !canonical.starts_with(&workspace_canonical) {
        return Err("Invalid path: escape detected".to_string());
    }

    Ok(file_path)
}

/// GET /api/v1/agents/{id}/files
pub async fn list_workspace_files(
    _state: State<AppState>,
    _claims: Claims,
    _workspace: WorkspaceScope,
    Path(_agent_id): Path<String>,
) -> Json<ApiResponse<WorkspaceFilesListResponse>> {
    let files: Vec<WorkspaceFileInfo> =
        WORKSPACE_FILES.iter().map(|name| WorkspaceFileInfo { name: name.to_string() }).collect();

    ApiResponseBuilder::success(WorkspaceFilesListResponse { files })
}

/// GET /api/v1/agents/{id}/files/{filename}
pub async fn get_workspace_file(
    state: State<AppState>,
    claims: Claims,
    workspace: WorkspaceScope,
    Path((_agent_id, filename)): Path<(String, String)>,
) -> Json<ApiResponse<WorkspaceFileResponse>> {
    let workspace_id = match workspace.0 {
        Some(ref id) => id.as_str(),
        None => return ApiResponseBuilder::error("X-Workspace-Id header required"),
    };

    // 验证 workspace 属于当前租户
    match state.workspace_service.find_by_id(workspace_id).await {
        Ok(Some(ws)) if ws.tenant_id != claims.tenant_id => {
            return ApiResponseBuilder::error_with_code(403, "无权访问此工作空间");
        }
        Ok(None) => {
            return ApiResponseBuilder::error_with_code(404, "工作空间不存在");
        }
        Err(e) => {
            tracing::error!("Failed to find workspace: {}", e);
            return ApiResponseBuilder::error("Failed to verify workspace");
        }
        _ => {}
    }

    let file_path = match validate_workspace_file_path(workspace_id, &filename) {
        Ok(path) => path,
        Err(_) => {
            return ApiResponseBuilder::error("Invalid file path");
        }
    };

    match tokio::fs::read_to_string(&file_path).await {
        Ok(content) => {
            ApiResponseBuilder::success(WorkspaceFileResponse { name: filename, content })
        }
        Err(_) => {
            // File doesn't exist - return empty content
            ApiResponseBuilder::success(WorkspaceFileResponse {
                name: filename,
                content: String::new(),
            })
        }
    }
}

/// PUT /api/v1/agents/{id}/files/{filename}
pub async fn put_workspace_file(
    state: State<AppState>,
    claims: Claims,
    workspace: WorkspaceScope,
    Path((_agent_id, filename)): Path<(String, String)>,
    Json(req): Json<UpdateWorkspaceFileRequest>,
) -> Json<ApiResponse<WorkspaceFileResponse>> {
    let workspace_id = match workspace.0 {
        Some(ref id) => id.as_str(),
        None => return ApiResponseBuilder::error("X-Workspace-Id header required"),
    };

    // 验证 workspace 属于当前租户
    match state.workspace_service.find_by_id(workspace_id).await {
        Ok(Some(ws)) if ws.tenant_id != claims.tenant_id => {
            return ApiResponseBuilder::error_with_code(403, "无权访问此工作空间");
        }
        Ok(None) => {
            return ApiResponseBuilder::error_with_code(404, "工作空间不存在");
        }
        Err(e) => {
            tracing::error!("Failed to find workspace: {}", e);
            return ApiResponseBuilder::error("Failed to verify workspace");
        }
        _ => {}
    }

    let file_path = match validate_workspace_file_path(workspace_id, &filename) {
        Ok(path) => path,
        Err(_) => {
            return ApiResponseBuilder::error("Invalid file path");
        }
    };

    // 文件大小限制
    if req.content.len() > MAX_FILE_SIZE {
        return ApiResponseBuilder::error("File too large (max 1MB)");
    }

    match tokio::fs::write(&file_path, &req.content).await {
        Ok(_) => ApiResponseBuilder::success(WorkspaceFileResponse {
            name: filename,
            content: req.content,
        }),
        Err(_) => ApiResponseBuilder::error("Failed to write file"),
    }
}
