//! 外部工具端口 + IoTToolAdapter（Task 14 自 apps/cloud `host/ports.rs` 与
//! `host/tools/service.rs` 迁入）。
//!
//! 组合层（MCP 平面）把它的 handler registry 适配到 [`ExternalToolRegistry`]
//! 并经 `ToolRegistry::set_external_tool_factory` 接线；本 crate 只面向 trait。

use std::sync::Arc;

use async_trait::async_trait;
use tinyiothub_skills::trust::{ToolSafety, classify_tool_safety};
use zeroclaw::tools::{Tool, ToolResult};
use zeroclaw_api::attribution::{Attributable, Role, ToolKind};

// ---------------------------------------------------------------------------
// External tool registry (MCP seam)
// ---------------------------------------------------------------------------

/// Metadata for an externally-registered tool (mirrors MCP `ToolMetadata`).
#[derive(Debug, Clone)]
pub struct ExternalToolMeta {
    pub name: String,
    pub description: String,
    pub input_schema: serde_json::Value,
}

/// Execution context for an external tool call: the workspace scope plus the
/// actor identity the composition layer should attribute the call to
/// (e.g. `"agent"` for chat turns, `"__heartbeat__:{ws}"` for heartbeat
/// approvals).
#[derive(Debug, Clone)]
pub struct ExternalToolContext {
    pub workspace_id: String,
    pub actor: String,
}

/// An externally-registered tool handler (mirrors the composition layer's
/// `ToolHandler` surface the agent runtime consumes).
#[async_trait]
pub trait ExternalToolHandler: Send + Sync {
    fn name(&self) -> &str;
    fn description(&self) -> &str;
    fn input_schema(&self) -> serde_json::Value;
    async fn execute(&self, ctx: &ExternalToolContext, args: serde_json::Value) -> Result<serde_json::Value, String>;

    /// Declared safety classification — authoritative for trust evaluation.
    fn safety(&self) -> ToolSafety {
        classify_tool_safety(self.name())
    }
}

/// Read-side registry of external tools available to agents.
#[async_trait]
pub trait ExternalToolRegistry: Send + Sync {
    async fn list_tools(&self) -> Vec<ExternalToolMeta>;
    async fn get_handler(&self, name: &str) -> Option<Arc<dyn ExternalToolHandler>>;
}

// ============================================================================
// IoTToolAdapter — wraps an external (MCP) tool handler as zeroclaw Tool
// ============================================================================

pub struct IoTToolAdapter {
    name: String,
    description: String,
    input_schema: serde_json::Value,
    handler: Arc<dyn ExternalToolHandler>,
    workspace_id: String,
    safety: ToolSafety,
}

impl IoTToolAdapter {
    pub fn new(
        name: String,
        description: String,
        input_schema: serde_json::Value,
        handler: Arc<dyn ExternalToolHandler>,
        workspace_id: String,
    ) -> Self {
        let safety = handler.safety();
        Self {
            name,
            description,
            input_schema,
            handler,
            workspace_id,
            safety,
        }
    }

    /// Handler-declared safety — authoritative for trust evaluation.
    pub fn safety(&self) -> ToolSafety {
        self.safety
    }
}

impl Attributable for IoTToolAdapter {
    fn role(&self) -> Role {
        Role::Tool(ToolKind::Plugin)
    }
    fn alias(&self) -> &str {
        <Self as Tool>::name(self)
    }
}

#[async_trait]
impl Tool for IoTToolAdapter {
    fn name(&self) -> &str {
        &self.name
    }

    fn description(&self) -> &str {
        &self.description
    }

    fn parameters_schema(&self) -> serde_json::Value {
        self.input_schema.clone()
    }

    async fn execute(&self, args: serde_json::Value) -> anyhow::Result<ToolResult> {
        // The composition layer's external-tool adapter applies the auth
        // context for this workspace/actor around the handler call.
        let ctx = ExternalToolContext {
            workspace_id: self.workspace_id.clone(),
            actor: "agent".to_string(),
        };
        match self.handler.execute(&ctx, args).await {
            Ok(output) => Ok(ToolResult {
                success: true,
                output: serde_json::to_string(&output).unwrap_or_default(),
                error: None,
            }),
            Err(err) => Ok(ToolResult {
                success: false,
                output: String::new(),
                error: Some(err.to_string()),
            }),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct SafetyDeclaringHandler {
        tool_name: &'static str,
        safety: Option<ToolSafety>,
    }

    #[async_trait]
    impl ExternalToolHandler for SafetyDeclaringHandler {
        fn name(&self) -> &str {
            self.tool_name
        }
        fn description(&self) -> &str {
            "stub handler"
        }
        fn input_schema(&self) -> serde_json::Value {
            serde_json::json!({"type": "object", "properties": {}})
        }
        async fn execute(
            &self,
            _ctx: &ExternalToolContext,
            _args: serde_json::Value,
        ) -> Result<serde_json::Value, String> {
            Ok(serde_json::json!({}))
        }
        fn safety(&self) -> ToolSafety {
            self.safety.unwrap_or_else(|| classify_tool_safety(self.name()))
        }
    }

    #[test]
    fn test_iot_tool_adapter_carries_handler_declared_safety() {
        let declared = SafetyDeclaringHandler {
            tool_name: "get_thing",
            safety: Some(ToolSafety::Destructive),
        };
        let adapter = IoTToolAdapter::new(
            declared.name().to_string(),
            declared.description().to_string(),
            declared.input_schema(),
            Arc::new(declared),
            "ws".to_string(),
        );
        assert_eq!(adapter.safety(), ToolSafety::Destructive);

        let defaulted = SafetyDeclaringHandler {
            tool_name: "get_thing",
            safety: None,
        };
        let adapter = IoTToolAdapter::new(
            defaulted.name().to_string(),
            defaulted.description().to_string(),
            defaulted.input_schema(),
            Arc::new(defaulted),
            "ws".to_string(),
        );
        // No declaration → name-pattern classification applies.
        assert_eq!(adapter.safety(), ToolSafety::ReadOnly);
    }
}
