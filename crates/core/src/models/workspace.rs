//! workspace 写入/请求契约类型（自 db 归位 core — handler 与 repo 共享的值类型）。

use std::fmt;

use serde::{Deserialize, Serialize};

/// Create workspace request
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct CreateWorkspaceRequest {
    pub name: String,
    pub description: Option<String>,
}

/// Update workspace request
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct UpdateWorkspaceRequest {
    pub name: Option<String>,
    pub description: Option<String>,
    pub agent_config: Option<String>,
    pub require_action_confirm: Option<bool>,
}

/// Assign device request
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct AssignDeviceRequest {
    pub device_id: String,
}

/// Create resource request
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct CreateResourceRequest {
    pub name: String,
    pub description: Option<String>,
    pub resource_type: ResourceType,
    pub content: Option<String>,
    pub tags: Vec<String>,
    pub metadata: Option<String>,
    pub file_path: Option<String>,
}

/// Update resource request
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct UpdateResourceRequest {
    pub name: Option<String>,
    pub description: Option<String>,
    pub content: Option<String>,
    pub tags: Option<Vec<String>>,
    pub metadata: Option<String>,
}

/// Suggest tags request
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct SuggestTagsRequest {
    pub name: String,
    pub resource_type: ResourceType,
    pub description: Option<String>,
}

/// Resource type: File (uploaded binaries) or Document (markdown knowledge).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ResourceType {
    File,
    Document,
}

impl ResourceType {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::File => "file",
            Self::Document => "document",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::File => "文件",
            Self::Document => "文档",
        }
    }

    pub fn from_string(s: &str) -> Option<Self> {
        match s {
            "file" => Some(Self::File),
            "document" => Some(Self::Document),
            _ => None,
        }
    }

    pub fn all() -> [Self; 2] {
        [Self::File, Self::Document]
    }
}

impl fmt::Display for ResourceType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}
