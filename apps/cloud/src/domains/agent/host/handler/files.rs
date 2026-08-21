// Workspace Files API for Agents
// HTTP endpoint handlers for agent workspace markdown files
//
// Architecture:
//   data/agents/_default/    ← Shared base (7 files, updated on deploy)
//   data/agents/{ws_id}/     ← Per-workspace overrides (USER.md, optional others)
//
// Loading priority: workspace dir → shared base (_default/) → embedded template
//
// Endpoints:
// - GET    /agents/{id}/files
// - GET    /agents/{id}/files/{filename}
// - PUT    /agents/{id}/files/{filename}
// - DELETE /agents/{id}/files/{filename}
//
// Supported files: IDENTITY.md, SOUL.md, AGENTS.md, USER.md, TOOLS.md, MEMORY.md, HEARTBEAT.md, BOOTSTRAP.md

use axum::{
    extract::{Path, State},
    response::Json,
};
use serde::{Deserialize, Serialize};
use tinyiothub_web::security::Claims;
use tinyiothub_web::response::ApiResponseBuilder;

use tinyiothub_web::api_response::ApiResponse;
use tinyiothub_web::middleware::workspace::WorkspaceScope;

use crate::domains::agent::AgentState;

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
#[serde(rename_all = "camelCase")]
pub struct WorkspaceFileInfo {
    pub name: String,
    /// Whether this file is a workspace-specific override (vs inherited from shared base)
    pub is_override: bool,
}

/// Response for workspace file content
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceFileResponse {
    pub name: String,
    pub content: String,
    /// Source of the content: "workspace", "shared", or "embedded"
    pub source: String,
}

/// Request for updating a workspace file
#[derive(Debug, Deserialize)]
pub struct UpdateWorkspaceFileRequest {
    pub content: String,
}

/// Maximum file size: 1MB (pub(crate) for testing)
pub(crate) const MAX_FILE_SIZE: usize = 1024 * 1024;

/// Resolve a file: returns (content, source) by checking workspace → shared base → embedded.
async fn resolve_file(workspace_id: &str, filename: &str) -> Result<(String, String), String> {
    let ws_dir = tinyiothub_agent::prompt::paths::workspace_dir(workspace_id);
    let shared_dir = tinyiothub_agent::prompt::paths::shared_agent_base_dir();

    // 1. Workspace override
    let ws_path = ws_dir.join(filename);
    if ws_path.exists()
        && let Ok(content) = tokio::fs::read_to_string(&ws_path).await
        && !content.trim().is_empty()
    {
        return Ok((content, "workspace".to_string()));
    }

    // 2. Shared base
    let shared_path = shared_dir.join(filename);
    if shared_path.exists()
        && let Ok(content) = tokio::fs::read_to_string(&shared_path).await
        && !content.trim().is_empty()
    {
        return Ok((content, "shared".to_string()));
    }

    // 3. Embedded template
    let embedded = get_embedded_template(filename);
    if let Some(content) = embedded
        && !content.trim().is_empty()
    {
        return Ok((content.to_string(), "embedded".to_string()));
    }

    Ok((String::new(), "embedded".to_string()))
}

fn get_embedded_template(filename: &str) -> Option<&'static str> {
    match filename {
        "IDENTITY.md" => Some(tinyiothub_agent::prompt::templates::IDENTITY_MD),
        "SOUL.md" => Some(tinyiothub_agent::prompt::templates::SOUL_MD),
        "AGENTS.md" => Some(tinyiothub_agent::prompt::templates::AGENTS_MD),
        "USER.md" => Some(tinyiothub_agent::prompt::templates::USER_MD),
        "TOOLS.md" => Some(tinyiothub_agent::prompt::templates::TOOLS_MD),
        "MEMORY.md" => Some(tinyiothub_agent::prompt::templates::MEMORY_MD),
        "HEARTBEAT.md" => Some(tinyiothub_agent::prompt::templates::HEARTBEAT_MD),
        "BOOTSTRAP.md" => Some(tinyiothub_agent::prompt::templates::BOOTSTRAP_MD),
        _ => None,
    }
}

async fn verify_workspace_access(state: &AgentState, workspace_id: &str, tenant_id: &str) -> Result<(), (u16, String)> {
    match state.workspace_access.workspace_tenant_id(workspace_id).await {
        Ok(Some(tid)) if tid != tenant_id => Err((403, "无权访问此工作空间".to_string())),
        Ok(None) => Err((404, "工作空间不存在".to_string())),
        Err(e) => {
            tracing::error!("Failed to find workspace: {}", e);
            Err((500, "Failed to verify workspace".to_string()))
        }
        _ => Ok(()),
    }
}

fn resolve_workspace_id(workspace: &WorkspaceScope) -> Result<&str, (u16, String)> {
    match workspace.0 {
        Some(ref id) => Ok(id.as_str()),
        None => Err((400, "X-Workspace-Id header required".to_string())),
    }
}

/// GET /api/v1/agents/{id}/files
/// Returns all 8 files with their source (workspace override, shared base, or embedded).
pub async fn list_workspace_files(
    state: State<AgentState>,
    claims: Claims,
    workspace: WorkspaceScope,
    Path(_agent_id): Path<String>,
) -> Json<ApiResponse<WorkspaceFilesListResponse>> {
    let workspace_id = match resolve_workspace_id(&workspace) {
        Ok(id) => id,
        Err((code, msg)) => return ApiResponseBuilder::error_with_code(code as i32, &msg),
    };

    if let Err((code, msg)) = verify_workspace_access(&state, workspace_id, &claims.tenant_id).await {
        return ApiResponseBuilder::error_with_code(code as i32, &msg);
    }

    let ws_dir = tinyiothub_agent::prompt::paths::workspace_dir(workspace_id);

    let mut files = Vec::new();
    for name in WORKSPACE_FILES {
        let is_override = ws_dir.join(name).exists();
        files.push(WorkspaceFileInfo {
            name: name.to_string(),
            is_override,
        });
    }

    ApiResponseBuilder::success(WorkspaceFilesListResponse { files })
}

/// GET /api/v1/agents/{id}/files/{filename}
/// Returns file content from workspace → shared base → embedded template, with source info.
pub async fn get_workspace_file(
    state: State<AgentState>,
    claims: Claims,
    workspace: WorkspaceScope,
    Path((_agent_id, filename)): Path<(String, String)>,
) -> Json<ApiResponse<WorkspaceFileResponse>> {
    let workspace_id = match resolve_workspace_id(&workspace) {
        Ok(id) => id,
        Err((code, msg)) => return ApiResponseBuilder::error_with_code(code as i32, &msg),
    };

    if !WORKSPACE_FILES.contains(&filename.as_str()) {
        return ApiResponseBuilder::error(format!(
            "Invalid filename: {}. Allowed: {:?}",
            filename, WORKSPACE_FILES
        ));
    }

    if let Err((code, msg)) = verify_workspace_access(&state, workspace_id, &claims.tenant_id).await {
        return ApiResponseBuilder::error_with_code(code as i32, &msg);
    }

    match resolve_file(workspace_id, &filename).await {
        Ok((content, source)) => ApiResponseBuilder::success(WorkspaceFileResponse {
            name: filename,
            content,
            source,
        }),
        Err(msg) => ApiResponseBuilder::error(&msg),
    }
}

/// PUT /api/v1/agents/{id}/files/{filename}
/// Always writes to the workspace directory (creates a workspace-specific override).
pub async fn put_workspace_file(
    state: State<AgentState>,
    claims: Claims,
    workspace: WorkspaceScope,
    Path((_agent_id, filename)): Path<(String, String)>,
    Json(req): Json<UpdateWorkspaceFileRequest>,
) -> Json<ApiResponse<WorkspaceFileResponse>> {
    let workspace_id = match resolve_workspace_id(&workspace) {
        Ok(id) => id,
        Err((code, msg)) => return ApiResponseBuilder::error_with_code(code as i32, &msg),
    };

    if !WORKSPACE_FILES.contains(&filename.as_str()) {
        return ApiResponseBuilder::error(format!(
            "Invalid filename: {}. Allowed: {:?}",
            filename, WORKSPACE_FILES
        ));
    }

    if let Err((code, msg)) = verify_workspace_access(&state, workspace_id, &claims.tenant_id).await {
        return ApiResponseBuilder::error_with_code(code as i32, &msg);
    }

    if req.content.len() > MAX_FILE_SIZE {
        return ApiResponseBuilder::error("File too large (max 1MB)");
    }

    let ws_dir = tinyiothub_agent::prompt::paths::workspace_dir(workspace_id);
    // Ensure workspace directory exists
    if !ws_dir.exists()
        && let Err(e) = tokio::fs::create_dir_all(&ws_dir).await
    {
        tracing::error!("Failed to create workspace dir: {}", e);
        return ApiResponseBuilder::error("Failed to create workspace directory");
    }

    let file_path = ws_dir.join(&filename);
    match tokio::fs::write(&file_path, &req.content).await {
        Ok(_) => ApiResponseBuilder::success(WorkspaceFileResponse {
            name: filename,
            content: req.content,
            source: "workspace".to_string(),
        }),
        Err(e) => {
            tracing::error!("Failed to write file {}: {}", filename, e);
            ApiResponseBuilder::error("Failed to write file")
        }
    }
}

/// DELETE /api/v1/agents/{id}/files/{filename}
/// Deletes the workspace-specific override, reverting to the shared base or embedded template.
pub async fn delete_workspace_file(
    state: State<AgentState>,
    claims: Claims,
    workspace: WorkspaceScope,
    Path((_agent_id, filename)): Path<(String, String)>,
) -> Json<ApiResponse<WorkspaceFileResponse>> {
    let workspace_id = match resolve_workspace_id(&workspace) {
        Ok(id) => id,
        Err((code, msg)) => return ApiResponseBuilder::error_with_code(code as i32, &msg),
    };

    if !WORKSPACE_FILES.contains(&filename.as_str()) {
        return ApiResponseBuilder::error(format!(
            "Invalid filename: {}. Allowed: {:?}",
            filename, WORKSPACE_FILES
        ));
    }

    if let Err((code, msg)) = verify_workspace_access(&state, workspace_id, &claims.tenant_id).await {
        return ApiResponseBuilder::error_with_code(code as i32, &msg);
    }

    let ws_dir = tinyiothub_agent::prompt::paths::workspace_dir(workspace_id);
    let file_path = ws_dir.join(&filename);

    if file_path.exists()
        && let Err(e) = tokio::fs::remove_file(&file_path).await
    {
        tracing::error!("Failed to delete file {}: {}", filename, e);
        return ApiResponseBuilder::error("Failed to delete file");
    }

    // Return the fallback content
    match resolve_file(workspace_id, &filename).await {
        Ok((content, source)) => ApiResponseBuilder::success(WorkspaceFileResponse {
            name: filename,
            content,
            source,
        }),
        Err(msg) => ApiResponseBuilder::error(&msg),
    }
}
