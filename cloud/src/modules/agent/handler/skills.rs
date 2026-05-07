// File-based skills CRUD — writes to data/agents/<workspace_id>/skills/<skill_name>.md

use std::path::PathBuf;

use axum::{
    Router,
    extract::{Path, State},
    http::StatusCode,
    response::Json,
    routing::get,
};
use serde::Deserialize;
use tinyiothub_web::response::ApiResponseBuilder;
use tokio::fs;

use crate::shared::{
    api_response::ApiResponse,
    app_state::AppState,
    paths::{self, global_skills_dir, workspace_skills_dir},
    security::jwt::Claims,
};

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

fn skill_file_path(workspace_id: &str, skill_name: &str) -> Result<PathBuf, String> {
    // Reject path traversal attempts
    if workspace_id.contains("..") || skill_name.contains("..") {
        return Err("Invalid path: traversal not allowed".to_string());
    }
    if workspace_id.starts_with('/') || skill_name.starts_with('/') {
        return Err("Invalid path: absolute paths not allowed".to_string());
    }

    // Workspace-specific skills: data/agents/<ws>/skills/<name>.md
    let dir = workspace_skills_dir(workspace_id);
    let file_path = dir.join(format!("{}.md", skill_name));

    // Verify the resolved path is still under the skills directory (defense in depth)
    for component in file_path.components() {
        if component == std::path::Component::ParentDir {
            return Err("Invalid path: escape detected".to_string());
        }
    }

    Ok(file_path)
}

pub fn create_router() -> Router<AppState> {
    Router::new()
        .route("/", get(list_skills).post(create_skill))
        .route("/{name}", get(get_skill).put(update_skill).delete(delete_skill))
}

fn resolve_workspace_id(claims: &Claims) -> &str {
    if claims.workspace_id.is_empty() { paths::DEFAULT_WORKSPACE_ID } else { &claims.workspace_id }
}

// GET /api/v1/agents/skills
pub async fn list_skills(
    _state: State<AppState>,
    claims: Claims,
) -> Json<ApiResponse<Vec<SkillInfoDto>>> {
    let workspace_id = resolve_workspace_id(&claims);
    // List workspace-specific skills
    let ws_skills = list_skill_files(&workspace_skills_dir(workspace_id)).await;
    // Also include global skills (read-only)
    let global = list_skill_files(&global_skills_dir()).await;

    let mut all_skills = ws_skills;
    for skill in global {
        if !all_skills.iter().any(|s| s.name == skill.name) {
            all_skills.push(skill);
        }
    }
    ApiResponseBuilder::success(all_skills)
}

// GET /api/v1/agents/skills/{name}
pub async fn get_skill(
    _state: State<AppState>,
    Path(name): Path<String>,
    claims: Claims,
) -> Result<Json<ApiResponse<SkillInfoDto>>, StatusCode> {
    let workspace_id = resolve_workspace_id(&claims);
    let file_path = skill_file_path(workspace_id, &name).map_err(|_| StatusCode::BAD_REQUEST)?;

    match fs::read_to_string(&file_path).await {
        Ok(content) => {
            let (fm, body) = crate::modules::agent::skill::AgentSkill::parse_frontmatter(&content);
            let skill_name = name.clone();
            let description = fm
                .as_ref()
                .and_then(|f| f.get("description"))
                .and_then(|v| v.as_str())
                .unwrap_or(&skill_name)
                .to_string();
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
    claims: Claims,
    Json(req): Json<CreateSkillRequest>,
) -> Result<Json<ApiResponse<SkillInfoDto>>, StatusCode> {
    let workspace_id = resolve_workspace_id(&claims);
    // Validate path
    let file_path =
        skill_file_path(workspace_id, &req.skill_name).map_err(|_| StatusCode::BAD_REQUEST)?;

    if file_path.exists() {
        return Err(StatusCode::CONFLICT); // File already exists
    }

    fs::create_dir_all(file_path.parent().unwrap())
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    fs::write(&file_path, &req.skill_content)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    tracing::info!("Skill saved: {}/{} -> {:?}", workspace_id, req.skill_name, file_path);

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
    claims: Claims,
    Json(req): Json<UpdateSkillRequest>,
) -> Result<Json<ApiResponse<SkillInfoDto>>, StatusCode> {
    let workspace_id = resolve_workspace_id(&claims);
    let file_path = skill_file_path(workspace_id, &name).map_err(|_| StatusCode::BAD_REQUEST)?;

    if fs::metadata(&file_path).await.is_err() {
        return Err(StatusCode::NOT_FOUND);
    }

    fs::write(&file_path, &req.skill_content)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    tracing::info!("Skill updated: {:?}", file_path);

    Ok(ApiResponseBuilder::success(SkillInfoDto {
        name: name.clone(),
        description: name.clone(),
        content: req.skill_content,
    }))
}

// DELETE /api/v1/agents/skills/{name}
pub async fn delete_skill(
    _state: State<AppState>,
    Path(name): Path<String>,
    claims: Claims,
) -> Result<Json<ApiResponse<()>>, StatusCode> {
    let workspace_id = resolve_workspace_id(&claims);
    let file_path = skill_file_path(workspace_id, &name).map_err(|_| StatusCode::BAD_REQUEST)?;

    if fs::metadata(&file_path).await.is_ok() {
        fs::remove_file(&file_path).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
        tracing::info!("Skill deleted: {:?}", file_path);
    }

    Ok(ApiResponseBuilder::success_with_message((), "Skill deleted"))
}

async fn list_skill_files(dir: &std::path::Path) -> Vec<SkillInfoDto> {
    let mut skills = Vec::new();
    if !dir.exists() {
        return skills;
    }

    let mut entries = match fs::read_dir(dir).await {
        Ok(e) => e,
        Err(_) => return skills,
    };

    while let Some(entry) = entries.next_entry().await.unwrap_or(None) {
        let path = entry.path();
        if path.extension().is_some_and(|e| e == "md") {
            let file_name = path.file_stem().unwrap().to_string_lossy().to_string();
            let content = fs::read_to_string(&path).await.unwrap_or_default();
            let (fm, body) = crate::modules::agent::skill::AgentSkill::parse_frontmatter(&content);
            let description = fm
                .as_ref()
                .and_then(|f| f.get("description"))
                .and_then(|v| v.as_str())
                .unwrap_or(&file_name)
                .to_string();
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
    use super::skill_file_path;
    use crate::modules::agent::skill::AgentSkill;

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
        assert!(skill_file_path("..", "foo").is_err());
        assert!(skill_file_path("tinyiothub", "../etc/passwd").is_err());
        assert!(skill_file_path("/", "foo").is_err());
        assert!(skill_file_path("tinyiothub", "/etc/passwd").is_err());
        assert!(skill_file_path("tinyiothub", "foo").is_ok());
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

        let mut entries: Vec<_> = fs::read_dir(&ws_dir)
            .unwrap()
            .filter_map(|e| e.ok())
            .filter(|e| e.path().extension().is_some_and(|ext| ext == "md"))
            .collect();
        entries.sort_by_key(|e| e.file_name().to_string_lossy().to_string());

        let names: Vec<_> = entries
            .iter()
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
