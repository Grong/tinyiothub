// File-based skills CRUD — writes to skills/<workspace_id>/<skill_name>.md

use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::Json,
    routing::{delete, get, post},
    Router,
};
use serde::Deserialize;
use std::sync::Mutex;
use std::path::PathBuf;

use crate::{api::AppState, dto::response::{api_response::ApiResponse, builder::ApiResponseBuilder}, shared::security::jwt::Claims};

#[derive(Debug, Deserialize)]
pub struct CreateSkillRequest {
    pub workspace_id: String,
    pub skill_name: String,
    pub skill_content: String,
}

#[derive(Debug, Deserialize)]
pub struct UpdateSkillRequest {
    pub skill_content: String,
}

// Mutex for concurrent file writes
lazy_static::lazy_static! {
    static ref SKILL_WRITE_MUTEX: Mutex<()> = Mutex::new(());
}

fn validate_skill_path(workspace_id: &str, skill_name: &str) -> Result<PathBuf, String> {
    // Reject path traversal attempts
    if workspace_id.contains("..") || skill_name.contains("..") {
        return Err("Invalid path: traversal not allowed".to_string());
    }
    if workspace_id.starts_with('/') || skill_name.starts_with('/') {
        return Err("Invalid path: absolute paths not allowed".to_string());
    }

    let base = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("skills");
    let file_path = base.join(workspace_id).join(format!("{}.md", skill_name));

    // Verify the resolved path is still under skills/ (defense in depth)
    let canonical = file_path.canonicalize().map_err(|_| "Invalid path")?;
    let skills_canonical = base.canonicalize().map_err(|_| "Invalid skills directory")?;
    if !canonical.starts_with(&skills_canonical) {
        return Err("Invalid path: escape detected".to_string());
    }

    Ok(file_path)
}

pub fn create_router() -> Router<AppState> {
    Router::new()
        .route("/", get(list_skills).post(create_skill))
        .route("/:name", get(get_skill).put(update_skill).delete(delete_skill))
}

// GET /api/v1/chat/skills?workspace_id=
pub async fn list_skills(
    _state: State<AppState>,
    _claims: Claims,
    axum::extract::Query(q): axum::extract::Query<ListSkillsQuery>,
) -> Json<ApiResponse<Vec<SkillInfoDto>>> {
    let workspace_id = q.workspace_id.as_deref().unwrap_or("tinyiothub");
    let skills_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("skills").join(workspace_id);

    let skills = list_skill_files(&skills_dir);
    ApiResponseBuilder::success(skills)
}

// GET /api/v1/chat/skills/:name?workspace_id=
pub async fn get_skill(
    _state: State<AppState>,
    Path(name): Path<String>,
    _claims: Claims,
    axum::extract::Query(q): axum::extract::Query<ListSkillsQuery>,
) -> Result<Json<ApiResponse<SkillInfoDto>>, StatusCode> {
    let workspace_id = q.workspace_id.as_deref().unwrap_or("tinyiothub");
    let file_path = validate_skill_path(workspace_id, &name)
        .map_err(|_| StatusCode::BAD_REQUEST)?;

    match std::fs::read_to_string(&file_path) {
        Ok(content) => {
            let (fm, body) = crate::domain::agent::skill::AgentSkill::parse_frontmatter(&content);
            let skill_name = name.clone();
            let description = fm.as_ref()
                .and_then(|f| f.get("description"))
                .and_then(|v| v.as_str())
                .unwrap_or(&skill_name).to_string();
            Ok(ApiResponseBuilder::success(SkillInfoDto {
                name: skill_name,
                description,
                content: body.trim().to_string(),
            }))
        }
        Err(_) => Err(StatusCode::NOT_FOUND),
    }
}

// POST /api/v1/chat/skills
pub async fn create_skill(
    _state: State<AppState>,
    _claims: Claims,
    Json(req): Json<CreateSkillRequest>,
) -> Result<Json<ApiResponse<SkillInfoDto>>, StatusCode> {
    // Validate path
    validate_skill_path(&req.workspace_id, &req.skill_name)
        .map_err(|e| StatusCode::BAD_REQUEST)?;

    let file_path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("skills")
        .join(&req.workspace_id)
        .join(format!("{}.md", req.skill_name));

    // Concurrent write guard
    let _guard = SKILL_WRITE_MUTEX.lock().map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    if file_path.exists() {
        return Err(StatusCode::CONFLICT); // File already exists
    }

    std::fs::create_dir_all(file_path.parent().unwrap())
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    // Disk-space check before write (skip if < 1MB available)
    if let Ok(metadata) = std::fs::metadata(file_path.parent().unwrap()) {
        if metadata.available_space() < req.skill_content.len() as u64 {
            tracing::warn!("Disk full when writing skill: {:?}", file_path);
            return Err(StatusCode::INSUFFICIENT_STORAGE);
        }
    }

    std::fs::write(&file_path, &req.skill_content)
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    tracing::info!("Skill saved: {}/{} -> {:?}", req.workspace_id, req.skill_name, file_path);

    Ok(ApiResponseBuilder::success_with_message(
        SkillInfoDto {
            name: req.skill_name.clone(),
            description: req.skill_name.clone(),
            content: req.skill_content,
        },
        "Skill created",
    ))
}

// PUT /api/v1/chat/skills/:name
pub async fn update_skill(
    _state: State<AppState>,
    Path(name): Path<String>,
    _claims: Claims,
    axum::extract::Query(q): axum::extract::Query<ListSkillsQuery>,
    Json(req): Json<UpdateSkillRequest>,
) -> Result<Json<ApiResponse<SkillInfoDto>>, StatusCode> {
    let workspace_id = q.workspace_id.as_deref().unwrap_or("tinyiothub");
    let file_path = validate_skill_path(workspace_id, &name)
        .map_err(|_| StatusCode::BAD_REQUEST)?;

    let _guard = SKILL_WRITE_MUTEX.lock().map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    if !file_path.exists() {
        return Err(StatusCode::NOT_FOUND);
    }

    std::fs::write(&file_path, &req.skill_content)
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    tracing::info!("Skill updated: {:?}", file_path);

    Ok(ApiResponseBuilder::success(SkillInfoDto {
        name: name.clone(),
        description: name.clone(),
        content: req.skill_content,
    }))
}

// DELETE /api/v1/chat/skills/:name?workspace_id=
pub async fn delete_skill(
    _state: State<AppState>,
    Path(name): Path<String>,
    _claims: Claims,
    axum::extract::Query(q): axum::extract::Query<ListSkillsQuery>,
) -> Result<Json<ApiResponse<()>>, StatusCode> {
    let workspace_id = q.workspace_id.as_deref().unwrap_or("tinyiothub");
    let file_path = validate_skill_path(workspace_id, &name)
        .map_err(|_| StatusCode::BAD_REQUEST)?;

    let _guard = SKILL_WRITE_MUTEX.lock().map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    if file_path.exists() {
        std::fs::remove_file(&file_path).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
        tracing::info!("Skill deleted: {:?}", file_path);
    }

    Ok(ApiResponseBuilder::success_with_message((), "Skill deleted"))
}

fn list_skill_files(dir: &std::path::Path) -> Vec<SkillInfoDto> {
    let mut skills = Vec::new();
    if !dir.exists() { return skills; }

    let entries = match std::fs::read_dir(dir) {
        Ok(e) => e,
        Err(_) => return skills,
    };

    for entry in entries.filter_map(|e| e.ok()) {
        let path = entry.path();
        if path.extension().map_or(false, |e| e == "md") {
            let file_name = path.file_stem().unwrap().to_string_lossy().to_string();
            let content = std::fs::read_to_string(&path).unwrap_or_default();
            let (fm, body) = crate::domain::agent::skill::AgentSkill::parse_frontmatter(&content);
            let description = fm.as_ref()
                .and_then(|f| f.get("description"))
                .and_then(|v| v.as_str())
                .unwrap_or(&file_name).to_string();
            skills.push(SkillInfoDto {
                name: file_name,
                description,
                content: body.trim().to_string(),
            });
        }
    }
    skills.sort_by_key(|s| s.name.clone());
    skills
}

#[derive(Debug, serde::Serialize)]
pub struct SkillInfoDto {
    pub name: String,
    pub description: String,
    pub content: String,
}

#[derive(Debug, Deserialize)]
pub struct ListSkillsQuery {
    pub workspace_id: Option<String>,
}
