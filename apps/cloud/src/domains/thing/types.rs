// Thing module types — DTOs, request/response types, and DB row
//
// DB 行类型（ThingRow/ThingResource/TagInfo/BreadcrumbNode/ThingTreeNode/
// EventRow/DocRow/ListThingsParams/UpdateThingRequest/UpdateGuardedOutcome）
// 已迁入 tinyiothub_storage::thing（Task 12），此处 re-export 保持既有路径。

use serde::{Deserialize, Serialize};

pub use tinyiothub_storage::thing::{
    BreadcrumbNode, DocRow, EventRow, ListThingsParams, TagInfo, ThingResource, ThingRow, ThingTreeNode,
    UpdateGuardedOutcome, UpdateThingRequest,
};

// ──────────────────────────────────────────────
// Enums
// ──────────────────────────────────────────────

/// The kind of Thing — supersedes the old "device" centric model.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ThingType {
    Device,
    Space,
    Line,
    Building,
    #[serde(untagged)]
    Custom(String),
}

impl ThingType {
    pub fn as_str(&self) -> &str {
        match self {
            ThingType::Device => "device",
            ThingType::Space => "space",
            ThingType::Line => "line",
            ThingType::Building => "building",
            ThingType::Custom(s) => s.as_str(),
        }
    }
}

impl std::str::FromStr for ThingType {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "device" => Ok(ThingType::Device),
            "space" => Ok(ThingType::Space),
            "line" => Ok(ThingType::Line),
            "building" => Ok(ThingType::Building),
            "" => Err("empty thing_type".to_string()),
            _ => Ok(ThingType::Custom(s.to_string())),
        }
    }
}

impl std::fmt::Display for ThingType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

/// Status of the AI-generated ontology summary for this thing.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SummaryStatus {
    Ok,
    Dirty,
    Failed,
}

impl SummaryStatus {
    pub fn as_str(&self) -> &str {
        match self {
            SummaryStatus::Ok => "ok",
            SummaryStatus::Dirty => "dirty",
            SummaryStatus::Failed => "failed",
        }
    }
}

impl std::str::FromStr for SummaryStatus {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "ok" => Ok(SummaryStatus::Ok),
            "dirty" => Ok(SummaryStatus::Dirty),
            "failed" => Ok(SummaryStatus::Failed),
            _ => Err(format!("unknown summary_status: {}", s)),
        }
    }
}

impl std::fmt::Display for SummaryStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

// ──────────────────────────────────────────────
// Response types
// ──────────────────────────────────────────────

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ThingResponse {
    pub id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub workspace_id: Option<String>,
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub display_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub category: Option<String>,
    pub thing_type: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parent_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub template_id: Option<String>,
    pub state: i32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub driver_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub protocol_type: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub address: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ontology_summary: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub summary_status: Option<String>,
    #[serde(default)]
    pub tags: Vec<TagInfo>,
    /// Ancestor chain from root to this node.
    pub breadcrumb: Vec<BreadcrumbNode>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ListThingsResult {
    pub items: Vec<ThingResponse>,
    pub total: u64,
    pub limit: u32,
    pub offset: u32,
    /// Count of resources not yet attached to any thing.
    pub unassigned_resource_count: u64,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ThingProfileResponse {
    #[serde(flatten)]
    pub thing: ThingResponse,
    /// Cached properties (from device_properties table).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub properties: Option<Vec<serde_json::Value>>,
    /// Available actions (from thing_templates.actions JSON).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub actions: Option<Vec<serde_json::Value>>,
    /// Recent events for this thing.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub recent_events: Option<Vec<serde_json::Value>>,
    /// Knowledge docs (from resources table).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub knowledge_docs: Option<Vec<serde_json::Value>>,
}

// ──────────────────────────────────────────────
// Request types
// ──────────────────────────────────────────────

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateThingRequest {
    pub name: String,
    #[serde(default)]
    pub thing_type: Option<String>,
    pub category: Option<String>,
    pub description: Option<String>,
    pub parent_id: Option<String>,
    pub template_id: Option<String>,
    pub protocol_type: Option<String>,
    pub driver_name: Option<String>,
    /// Tags: comma-separated or JSON array string.
    pub tags: Option<String>,
    pub workspace_id: Option<String>,
}
