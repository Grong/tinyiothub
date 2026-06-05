// SearchWorkspaceResourcesTool — Agent tool for semantic resource search

use std::sync::Arc;

use async_trait::async_trait;
use zeroclaw::tools::{Tool, ToolResult};

use crate::modules::workspace::{WorkspaceService, types::ResourceType};

pub struct SearchWorkspaceResourcesTool {
    workspace_service: Arc<WorkspaceService>,
}

impl SearchWorkspaceResourcesTool {
    pub fn new(workspace_service: Arc<WorkspaceService>) -> Self {
        Self { workspace_service }
    }
}

#[async_trait]
impl Tool for SearchWorkspaceResourcesTool {
    fn name(&self) -> &str {
        "search_workspace_resources"
    }

    fn description(&self) -> &str {
        "Search workspace multimedia resources (3D scenes, images, documents) by natural language query. \
         Returns matching resources with relevance scores. \
         Use this when the user asks about 3D scenes, building layouts, or spatial information."
    }

    fn parameters_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "workspace_id": {
                    "type": "string",
                    "description": "The workspace ID to search within"
                },
                "query": {
                    "type": "string",
                    "description": "Natural language search query, e.g. '3楼车间温度传感器' or 'factory floor plan'"
                },
                "resource_type": {
                    "type": "string",
                    "description": "Optional filter by resource type.",
                    "enum": ResourceType::all().iter().map(|rt| rt.as_str()).collect::<Vec<_>>(),
                },
                "limit": {
                    "type": "integer",
                    "description": "Maximum results (default 10, max 50)",
                    "minimum": 1,
                    "maximum": 50,
                    "default": 10
                }
            },
            "required": ["workspace_id", "query"],
        })
    }

    async fn execute(&self, args: serde_json::Value) -> anyhow::Result<ToolResult> {
        let workspace_id = args.get("workspace_id").and_then(|v| v.as_str()).unwrap_or("");

        if workspace_id.is_empty() {
            return Ok(ToolResult {
                success: false,
                output: String::new(),
                error: Some("workspace_id is required".into()),
            });
        }

        let query = args.get("query").and_then(|v| v.as_str()).unwrap_or("");

        let resource_type =
            args.get("resource_type").and_then(|v| v.as_str()).and_then(ResourceType::from_str);

        let limit = args.get("limit").and_then(|v| v.as_i64()).unwrap_or(10).clamp(1, 50);

        match self
            .workspace_service
            .search_resources(workspace_id, query, resource_type, limit)
            .await
        {
            Ok(results) => {
                let output = serde_json::json!({
                    "resources": results,
                    "count": results.len(),
                    "query": query,
                });
                Ok(ToolResult {
                    success: true,
                    output: serde_json::to_string(&output).unwrap_or_default(),
                    error: None,
                })
            }
            Err(e) => Ok(ToolResult {
                success: false,
                output: String::new(),
                error: Some(format!("Search failed: {}", e)),
            }),
        }
    }
}
