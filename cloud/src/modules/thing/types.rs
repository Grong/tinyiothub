// Thing module types — DTOs, request/response types, and DB row

use serde::{Deserialize, Serialize};
use sqlx::FromRow;

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
// DB Row
// ──────────────────────────────────────────────

/// Maps to the `devices` table after the Thing Ontology migration.
#[derive(Debug, Clone, FromRow)]
pub struct ThingRow {
    pub id: String,
    pub name: String,
    pub display_name: Option<String>,
    pub thing_type: String,
    pub device_type: Option<String>,
    pub address: Option<String>,
    pub description: Option<String>,
    pub position: Option<String>,
    pub driver_name: Option<String>,
    pub device_model: Option<String>,
    pub protocol_type: Option<String>,
    pub factory_name: Option<String>,
    pub linked_data: Option<String>,
    pub driver_options: Option<String>,
    pub state: i32,
    pub parent_id: Option<String>,
    pub organization_id: Option<String>,
    pub tenant_id: Option<String>,
    pub workspace_id: Option<String>,
    pub linked_gateway: Option<String>,
    pub fingerprint: Option<String>,
    pub template_id: Option<String>,
    pub ontology_summary: Option<String>,
    pub summary_status: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

// ──────────────────────────────────────────────
// Query params
// ──────────────────────────────────────────────

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ListThingsParams {
    pub thing_type: Option<String>,
    pub parent_id: Option<String>,
    pub tags: Option<String>,
    pub q: Option<String>,
    pub limit: Option<u32>,
    pub offset: Option<u32>,
}

impl ListThingsParams {
    /// Clamp `limit` to 1..=200, default 50.
    pub fn limit(&self) -> u32 {
        let raw = self.limit.unwrap_or(50);
        raw.clamp(1, 200)
    }

    /// Default offset to 0.
    pub fn offset(&self) -> u32 {
        self.offset.unwrap_or(0)
    }
}

// ──────────────────────────────────────────────
// Response types
// ──────────────────────────────────────────────

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BreadcrumbNode {
    pub id: String,
    pub name: String,
    pub thing_type: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ThingResponse {
    pub id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub workspace_id: Option<String>,
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub device_type: Option<String>,
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
    pub ontology_summary: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub summary_status: Option<String>,
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
    /// Recent events for this thing.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub recent_events: Option<Vec<serde_json::Value>>,
    /// Knowledge docs (from resources table).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub knowledge_docs: Option<Vec<serde_json::Value>>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ThingTreeNode {
    pub id: String,
    pub name: String,
    pub thing_type: String,
    pub children: Vec<ThingTreeNode>,
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
    pub device_type: Option<String>,
    pub description: Option<String>,
    pub parent_id: Option<String>,
    pub template_id: Option<String>,
    pub protocol_type: Option<String>,
    pub driver_name: Option<String>,
    /// Tags: comma-separated or JSON array string.
    pub tags: Option<String>,
    pub workspace_id: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateThingRequest {
    pub name: Option<String>,
    pub thing_type: Option<String>,
    pub device_type: Option<String>,
    pub description: Option<String>,
    pub parent_id: Option<String>,
    pub template_id: Option<String>,
    pub protocol_type: Option<String>,
    pub driver_name: Option<String>,
    pub tags: Option<String>,
}

// ──────────────────────────────────────────────
// Resource types
// ──────────────────────────────────────────────

/// Maps to the `resources` table (formerly `thing_resources`).
#[derive(Debug, Clone, FromRow, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ThingResource {
    pub id: String,
    pub workspace_id: String,
    pub device_id: Option<String>,
    #[sqlx(rename = "type")]
    pub resource_type: String,
    pub name: String,
    pub file_path: String,
    pub content: Option<String>,
    pub tags: String,
    pub created_at: String,
    pub updated_at: String,
}
