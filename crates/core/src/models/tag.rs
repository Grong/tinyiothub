//! tag 写入/请求契约类型（自 db 归位 core — handler 与 repo 共享的值类型）。

use serde::{Deserialize, Serialize};

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
