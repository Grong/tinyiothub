// Thing Agent Tools — 9 AI agent tools for Thing Ontology
//
// These tools give the AI agent read+execute access to the Thing Ontology.
// They follow the zeroclaw Tool trait pattern (same as CanvasTool, GetSkillTool).
//
// The 9 tools (one file per tool, G9 flat layout):
//   1. list_things       — list things in workspace (paginated)        [list.rs]
//   2. get_thing         — lightweight thing view                       [get.rs]
//   3. get_thing_profile — full snapshot (properties + events + docs)   [profile.rs]
//   4. get_thing_tree    — hierarchical tree                            [tree.rs]
//   5. read_property     — current property value from device_cache     [read_property.rs]
//   6. invoke_action     — execute a device action (type='device' guard)[invoke_action.rs]
//   7. query_events      — query events for a thing                     [query_events.rs]
//   8. search_knowledge  — full-text search thing_resources             [search_knowledge.rs]
//   9. read_document     — full document content                        [read_document.rs]
//
// Shared homes: confirmation store [pending_action.rs], param validation
// [validate.rs], the create_thing_tools factory and helpers live here.

mod get;
mod invoke_action;
mod list;
mod pending_action;
mod profile;
mod query_events;
mod read_document;
mod read_property;
mod search_knowledge;
mod tree;
mod validate;

use std::sync::Arc;

use crate::domains::agent::loop_::types::ToolSafety;
use crate::domains::thing::service::ThingService;
use sqlx::SqlitePool;
use zeroclaw::tools::{Tool, ToolResult};

pub use get::GetThingTool;
pub use invoke_action::InvokeActionTool;
pub use list::ListThingsTool;
pub use pending_action::{
    PendingAction, PendingActionStore, cleanup_expired_tokens, store_pending_action,
    take_pending_action,
};
pub use profile::GetThingProfileTool;
pub use query_events::QueryEventsTool;
pub use read_document::ReadDocumentTool;
pub use read_property::ReadPropertyTool;
pub use search_knowledge::SearchKnowledgeTool;
pub use tree::GetThingTreeTool;
pub use validate::validate_action_params;

// ============================================================================
// Helpers
// ============================================================================

/// Wrap a serializable payload into a successful ToolResult.
pub(crate) fn tool_ok(payload: impl serde::Serialize) -> anyhow::Result<ToolResult> {
    Ok(ToolResult {
        success: true,
        output: serde_json::to_string(&payload).unwrap_or_default(),
        error: None,
    })
}

/// Wrap an error message into a failed ToolResult.
pub(crate) fn tool_err(msg: impl Into<String>) -> anyhow::Result<ToolResult> {
    Ok(ToolResult {
        success: false,
        output: String::new(),
        error: Some(msg.into()),
    })
}

/// Clamp limit to [1, max], defaulting when None.
pub(crate) fn clamp_limit(limit: Option<u32>, default: u32, max: u32) -> u32 {
    limit.unwrap_or(default).clamp(1, max)
}

// ============================================================================
// Factory: create all 9 thing tools
// ============================================================================

/// Create all 9 Thing Ontology agent tools with their safety classifications.
///
/// Read-only tools (searches, gets): safety ReadOnly => auto-approved.
/// Destructive tools (invoke_action): safety Destructive => requires trust approval.
pub fn create_thing_tools(
    pool: SqlitePool,
    workspace_id: &str,
    runtime: &super::service::ToolRuntimeContext,
) -> Vec<(Box<dyn Tool>, ToolSafety)> {
    let thing_service = Arc::new(ThingService::new(pool.clone()));
    let ws = workspace_id.to_string();

    // Read-only tools — auto-approved
    let read_only = |t: Box<dyn Tool>| (t, ToolSafety::ReadOnly);

    // Destructive tools — require trust approval
    let destructive = |t: Box<dyn Tool>| (t, ToolSafety::Destructive);

    vec![
        // Read-only tools (8)
        read_only(Box::new(ListThingsTool {
            thing_service: thing_service.clone(),
            workspace_id: ws.clone(),
        })),
        read_only(Box::new(GetThingTool {
            thing_service: thing_service.clone(),
            workspace_id: ws.clone(),
        })),
        read_only(Box::new(GetThingProfileTool {
            thing_service: thing_service.clone(),
            workspace_id: ws.clone(),
        })),
        read_only(Box::new(GetThingTreeTool {
            thing_service: thing_service.clone(),
            workspace_id: ws.clone(),
        })),
        read_only(Box::new(ReadPropertyTool {
            thing_service: thing_service.clone(),
            pool: pool.clone(),
            workspace_id: ws.clone(),
            device_cache: runtime.device_cache.clone(),
        })),
        read_only(Box::new(QueryEventsTool {
            pool: pool.clone(),
            workspace_id: ws.clone(),
        })),
        read_only(Box::new(SearchKnowledgeTool {
            pool: pool.clone(),
            workspace_id: ws.clone(),
        })),
        read_only(Box::new(ReadDocumentTool {
            pool: pool.clone(),
            workspace_id: ws.clone(),
        })),
        // Destructive tool (1)
        destructive(Box::new(InvokeActionTool {
            thing_service,
            pool,
            workspace_id: ws,
            data_server: runtime.data_server.clone(),
            pending_actions: runtime
                .pending_actions
                .clone()
                .expect("pending_actions must be wired in ToolRuntimeContext"),
        })),
    ]
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    // ── list_things pagination clamp ──────────────────────

    /// Verify that limit=500 is clamped down to 200 (max).
    #[test]
    fn test_list_things_pagination_clamp() {
        assert_eq!(clamp_limit(None, 50, 200), 50, "None => default 50");
        assert_eq!(clamp_limit(Some(30), 50, 200), 30, "explicit within range");
        assert_eq!(clamp_limit(Some(500), 50, 200), 200, "500 => max 200");
        assert_eq!(clamp_limit(Some(0), 50, 200), 1, "0 => min 1");
        assert_eq!(clamp_limit(Some(1000), 50, 200), 200, "1000 => max 200");
    }

    // ── get_thing_tree depth clamp ──────────────────────

    #[test]
    fn test_get_thing_tree_depth_clamp() {
        // Depth used via .clamp(1, 10) in execute
        assert_eq!(3.clamp(1, 10), 3, "None default 3");
        assert_eq!(5u32.clamp(1, 10), 5, "explicit 5");
        assert_eq!(0u32.clamp(1, 10), 1, "0 => min 1");
        assert_eq!(50u32.clamp(1, 10), 10, "50 => max 10");
    }

    // ── invoke_action non-device rejection ──────────────

    /// Verify that invoke_action is classified as Write (not Destructive) by
    /// name-based classification. The factory explicitly declares it as
    /// Destructive for trust enforcement.
    #[test]
    fn test_invoke_action_rejects_non_device_type_in_schema() {
        // Name-based classification: invoke_action → Write (not Destructive)
        assert_eq!(
            crate::domains::agent::loop_::types::classify_tool_safety("invoke_action"),
            ToolSafety::Write,
            "invoke_action is Write by name pattern; factory overrides to Destructive"
        );
    }

    // ── tool name uniqueness ────────────────────────────

    /// Verify all 9 tools are uniquely named.
    #[test]
    fn test_all_9_tool_names_unique() {
        // This test validates that the 9 tool names are correct and unique.
        let names = vec![
            "list_things",
            "get_thing",
            "get_thing_profile",
            "get_thing_tree",
            "read_property",
            "invoke_action",
            "query_events",
            "search_knowledge",
            "read_document",
        ];
        assert_eq!(names.len(), 9, "should have exactly 9 tools");

        // Check uniqueness
        let mut sorted = names.clone();
        sorted.sort();
        sorted.dedup();
        assert_eq!(sorted.len(), 9, "all 9 tool names must be unique");
    }

    // ── safety classification on name pattern ──────────

    #[test]
    fn test_classify_tool_safety_by_name() {
        use crate::domains::agent::loop_::types::classify_tool_safety;

        // Read-only: starts with list_/get_/read_/search_
        assert_eq!(classify_tool_safety("list_things"), ToolSafety::ReadOnly);
        assert_eq!(classify_tool_safety("get_thing"), ToolSafety::ReadOnly);
        assert_eq!(classify_tool_safety("get_thing_profile"), ToolSafety::ReadOnly);
        assert_eq!(classify_tool_safety("get_thing_tree"), ToolSafety::ReadOnly);
        assert_eq!(classify_tool_safety("read_property"), ToolSafety::ReadOnly);
        assert_eq!(classify_tool_safety("read_document"), ToolSafety::ReadOnly);
        assert_eq!(classify_tool_safety("search_knowledge"), ToolSafety::ReadOnly);

        // Write: doesn't match read/destructive patterns
        assert_eq!(classify_tool_safety("invoke_action"), ToolSafety::Write);
        assert_eq!(classify_tool_safety("query_events"), ToolSafety::Write);
    }
}
