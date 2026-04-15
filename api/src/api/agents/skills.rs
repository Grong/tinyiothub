// File-based skills CRUD — writes to skills/<workspace_id>/<skill_name>.md

use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::Json,
    routing::get,
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
    // Use components() to avoid needing the file to exist
    for component in file_path.components() {
        match component {
            std::path::Component::ParentDir => {
                return Err("Invalid path: escape detected".to_string());
            }
            _ => {}
        }
    }

    Ok(file_path)
}

pub fn create_router() -> Router<AppState> {
    Router::new()
        .route("/", get(list_skills).post(create_skill))
        .route("/{name}", get(get_skill).put(update_skill).delete(delete_skill))
}

// GET /api/v1/agents/skills?workspace_id=
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

// GET /api/v1/agents/skills/{name}?workspace_id=
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

// POST /api/v1/agents/skills
pub async fn create_skill(
    _state: State<AppState>,
    _claims: Claims,
    Json(req): Json<CreateSkillRequest>,
) -> Result<Json<ApiResponse<SkillInfoDto>>, StatusCode> {
    // Validate path
    validate_skill_path(&req.workspace_id, &req.skill_name)
        .map_err(|_e| StatusCode::BAD_REQUEST)?;

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

// PUT /api/v1/agents/skills/{name}
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

// DELETE /api/v1/agents/skills/{name}?workspace_id=
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

#[cfg(test)]
mod tests {
    use super::validate_skill_path;
    use crate::domain::agent::skill::AgentSkill;

    #[test]
    fn skill_with_frontmatter_parsing() {
        let content = r#"---
name: alarm-management
description: Manage alarms
version: 1.0.0
---

# Alarm Management"#;

        let (fm, body) = AgentSkill::parse_frontmatter(content);
        assert!(fm.is_some());
        let fm = fm.unwrap();
        assert_eq!(fm.get("name").unwrap().as_str().unwrap(), "alarm-management");
        assert_eq!(fm.get("description").unwrap().as_str().unwrap(), "Manage alarms");
        assert_eq!(fm.get("version").unwrap().as_str().unwrap(), "1.0.0");
        assert!(body.contains("Alarm Management"));
    }

    #[test]
    fn skill_without_frontmatter() {
        let content = "# Plain skill without frontmatter";
        let (fm, body) = AgentSkill::parse_frontmatter(content);
        assert!(fm.is_none());
        assert_eq!(body.trim(), "# Plain skill without frontmatter");
    }

    #[test]
    fn skill_parse_returns_correct_body() {
        let content = r#"---
name: test
description: desc
---

# Title

Some body content here."#;

        let (_fm, body) = AgentSkill::parse_frontmatter(content);
        let body = body.trim();
        assert!(body.starts_with("# Title"));
        assert!(body.contains("Some body content here."));
    }

    #[test]
    fn skill_name_from_filename() {
        let file_name = "device-onboarding.md";
        let skill_name = file_name.trim_end_matches(".md");
        assert_eq!(skill_name, "device-onboarding");
    }

    #[test]
    fn skill_name_with_dashes_and_underscores() {
        let file_name = "device-onboarding_v2.md";
        let skill_name = file_name.trim_end_matches(".md");
        assert_eq!(skill_name, "device-onboarding_v2");
    }

    #[test]
    fn path_traversal_rejected_in_validation() {
        assert!(validate_skill_path("..", "foo").is_err());
        assert!(validate_skill_path("tinyiothub", "../etc/passwd").is_err());
        assert!(validate_skill_path("/", "foo").is_err());
        assert!(validate_skill_path("tinyiothub", "/etc/passwd").is_err());
        assert!(validate_skill_path("tinyiothub", "foo").is_ok());
    }

    #[test]
    fn skill_files_sorted_deterministically() {
        use std::fs;
        use tempfile::TempDir;

        let dir = TempDir::new().unwrap();
        let ws_dir = dir.path().join("tinyiothub");
        fs::create_dir_all(&ws_dir).unwrap();

        fs::write(ws_dir.join("z-skill.md"), "---\nname: z\n---\n\nZ").unwrap();
        fs::write(ws_dir.join("a-skill.md"), "---\nname: a\n---\n\nA").unwrap();
        fs::write(ws_dir.join("m-skill.md"), "---\nname: m\n---\n\nM").unwrap();

        let mut entries: Vec<_> = fs::read_dir(&ws_dir).unwrap()
            .filter_map(|e| e.ok())
            .filter(|e| e.path().extension().map_or(false, |ext| ext == "md"))
            .collect();
        entries.sort_by_key(|e| e.file_name().to_string_lossy().to_string());

        let names: Vec<_> = entries.iter()
            .map(|e| e.file_name().to_string_lossy().trim_end_matches(".md").to_string())
            .collect();
        assert_eq!(names, vec!["a-skill", "m-skill", "z-skill"]);
    }

    #[test]
    fn create_and_read_skill_file() {
        use std::fs;
        use tempfile::TempDir;

        let dir = TempDir::new().unwrap();
        let ws_dir = dir.path().join("tinyiothub");
        fs::create_dir_all(&ws_dir).unwrap();

        let file_path = ws_dir.join("test-skill.md");
        let content = "---\nname: test\ndescription: Test skill\n---\n\n# Test";
        fs::write(&file_path, content).unwrap();

        let read = fs::read_to_string(&file_path).unwrap();
        assert!(read.contains("Test skill"));
    }

    #[test]
    fn skill_file_yaml_roundtrip() {
        use std::fs;
        use tempfile::TempDir;

        let dir = TempDir::new().unwrap();
        let ws_dir = dir.path().join("tinyiothub");
        fs::create_dir_all(&ws_dir).unwrap();

        let file_path = ws_dir.join("alarm-management.md");
        let content = r#"---
name: alarm-management
description: Manage alarms
version: 1.0.0
---

# Alarm Management

Some detailed content here."#;

        fs::write(&file_path, content).unwrap();

        let read = fs::read_to_string(&file_path).unwrap();
        let (fm, body) = AgentSkill::parse_frontmatter(&read);

        assert!(fm.is_some());
        let fm = fm.unwrap();
        assert_eq!(fm.get("name").unwrap().as_str().unwrap(), "alarm-management");
        assert!(body.trim().contains("Alarm Management"));
    }

}
