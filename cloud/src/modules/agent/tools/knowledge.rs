// SearchKnowledgeTool — Agent tool for knowledge graph search

use std::sync::Arc;

use async_trait::async_trait;
use zeroclaw::tools::{Tool, ToolResult};

use crate::modules::workspace::KnowledgeService;

pub struct SearchKnowledgeTool {
    knowledge_service: Arc<KnowledgeService>,
}

impl SearchKnowledgeTool {
    pub fn new(knowledge_service: Arc<KnowledgeService>) -> Self {
        Self { knowledge_service }
    }
}

#[async_trait]
impl Tool for SearchKnowledgeTool {
    fn name(&self) -> &str {
        "search_knowledge"
    }

    fn description(&self) -> &str {
        "Search the workspace knowledge graph for entities, concepts, relationships, and context. \
         Returns matching entities with their relations and relevance scores. \
         Use this when the user asks about workspace-specific knowledge, stored documents, \
         IoT entities (spaces, devices), or any domain-specific information that might be in the knowledge base."
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
                    "description": "Natural language search query, e.g. '车间温度传感器' or 'building layout'"
                },
                "entity_type": {
                    "type": "string",
                    "enum": ["space", "device", "functional"],
                    "description": "Optional filter by entity type"
                },
                "tags": {
                    "type": "array",
                    "items": { "type": "string" },
                    "description": "Optional filter by tags"
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
        let workspace_id = args
            .get("workspace_id")
            .and_then(|v| v.as_str())
            .unwrap_or("");

        if workspace_id.is_empty() {
            return Ok(ToolResult {
                success: false,
                output: String::new(),
                error: Some("workspace_id is required".into()),
            });
        }

        let query = args.get("query").and_then(|v| v.as_str()).unwrap_or("");

        if query.is_empty() {
            return Ok(ToolResult {
                success: false,
                output: String::new(),
                error: Some("query is required".into()),
            });
        }

        let entity_type = args.get("entity_type").and_then(|v| v.as_str());

        let tags: Option<String> = args.get("tags").and_then(|v| v.as_array()).map(|arr| {
            arr.iter()
                .filter_map(|t| t.as_str())
                .collect::<Vec<_>>()
                .join(",")
        });

        let limit = args
            .get("limit")
            .and_then(|v| v.as_i64())
            .unwrap_or(10)
            .clamp(1, 50);

        match self
            .knowledge_service
            .search_knowledge(
                workspace_id,
                query,
                entity_type,
                tags.as_deref(),
                limit,
            )
            .await
        {
            Ok(results) => {
                if results.is_empty() {
                    return Ok(ToolResult {
                        success: true,
                        output: format!(
                            "No knowledge graph results found for query: \"{}\"",
                            query
                        ),
                        error: None,
                    });
                }

                let formatted: Vec<serde_json::Value> = results
                    .iter()
                    .map(|r| {
                        let entity = &r.entity;
                        let mut relations_summary: Vec<String> = r
                            .relations
                            .iter()
                            .map(|rel| {
                                format!("{} -> {} ({})", rel.relation_type, rel.target_entity_id, rel.confidence)
                            })
                            .collect();
                        relations_summary.truncate(5);

                        serde_json::json!({
                            "name": entity.name,
                            "entity_type": entity.entity_type,
                            "description": entity.description,
                            "tags": entity.tags,
                            "confidence": entity.confidence,
                            "relevance": r.relevance,
                            "source_snippet": r.source_snippet,
                            "relation_count": r.relations.len(),
                        })
                    })
                    .collect();

                let output = serde_json::json!({
                    "results": formatted,
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
                error: Some(format!("Knowledge search failed: {}", e)),
            }),
        }
    }
}
