use serde::{Deserialize, Serialize};
use sqlx::FromRow;

/// Tag entity - 标签实体
#[derive(Debug, Clone, FromRow, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct Tag {
    pub id: String,
    #[serde(rename = "type")]
    pub tag_type: String, // "device" or "app"
    pub name: String,
    pub tenant_id: Option<String>,
    pub created_by: Option<String>,
    pub created_at: String,
}

/// Tag binding entity - 标签绑定实体
#[derive(Debug, Clone, FromRow, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct TagBinding {
    pub id: String,
    pub tag_id: String,
    pub target_id: String,
    pub tenant_id: Option<String>,
    pub created_by: Option<String>,
    pub created_at: String,
}

/// Query parameters for tag search
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub struct TagQuery {
    pub name: Option<String>,
    #[serde(rename = "type")]
    pub tag_type: Option<String>,
    pub target_id: Option<String>,
    pub tenant_id: Option<String>,
    pub page: Option<u32>,
    pub page_size: Option<u32>,
}

/// Request for creating a new tag
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct CreateTagRequest {
    pub name: String,
    #[serde(rename = "type")]
    pub tag_type: String, // "device" or "app"
}

/// Request for updating a tag
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct UpdateTagRequest {
    pub name: Option<String>,
}

/// Request for creating a tag binding
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct CreateTagBindingRequest {
    pub tag_id: String,
    pub target_id: String,
    pub target_type: String,
}

/// Request for batch creating tag bindings
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct BatchTagBindingRequest {
    pub tag_ids: Vec<String>,
    pub target_id: String,
    pub target_type: String,
}
