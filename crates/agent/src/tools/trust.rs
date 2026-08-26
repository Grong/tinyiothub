//! TrustAwareTool — 以信任等级包装 Tool 的执行期强制（Task 14 自 apps/cloud
//! `host/tools/service.rs` 迁入）。

use std::sync::Arc;

use async_trait::async_trait;
use tinyiothub_core::heartbeat::TrustConfig;
use tinyiothub_skills::trust::{ToolSafety, TrustDecision};
use zeroclaw::tools::{Tool, ToolResult};
use zeroclaw_api::attribution::{Attributable, Role};

/// Proxies a `Box<dyn Tool>`, delegating trust evaluation to the unified
/// policy engine adapter.
///
/// Trust decision comes from the policy crate — tool metadata (read/destructive)
/// is authoritative; the TrustConfig only provides overrides.
pub struct TrustAwareTool {
    inner: Box<dyn Tool>,
    trust_config: Arc<TrustConfig>,
    safety: ToolSafety,
}

impl TrustAwareTool {
    pub fn new(inner: Box<dyn Tool>, trust_config: Arc<TrustConfig>, safety: ToolSafety) -> Self {
        Self {
            inner,
            trust_config,
            safety,
        }
    }
}

impl Attributable for TrustAwareTool {
    fn role(&self) -> Role {
        self.inner.role()
    }
    fn alias(&self) -> &str {
        self.inner.alias()
    }
}

#[async_trait]
impl Tool for TrustAwareTool {
    fn name(&self) -> &str {
        self.inner.name()
    }

    fn description(&self) -> &str {
        self.inner.description()
    }

    fn parameters_schema(&self) -> serde_json::Value {
        self.inner.parameters_schema()
    }

    async fn execute(&self, args: serde_json::Value) -> anyhow::Result<ToolResult> {
        let tool_name = <Self as Tool>::name(self);

        // X3/T16: the legacy heartbeat trust path is converged onto the unified
        // engine via HeartbeatTrustAdapter. O23 equivalence: for the same
        // TrustConfig input the adapter's verdict equals
        // evaluate_tool_trust_with_safety (verified by the adapter's
        // parameterized equivalence tests).
        match tinyiothub_policy::adapters::HeartbeatTrustAdapter::evaluate(&self.trust_config, tool_name, self.safety) {
            TrustDecision::Allow => self.inner.execute(args).await,
            TrustDecision::Block { reason } => Ok(ToolResult {
                success: false,
                output: String::new(),
                error: Some(reason),
            }),
            TrustDecision::Propose { reason } => Ok(ToolResult {
                success: false,
                output: String::new(),
                error: Some(reason),
            }),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use zeroclaw_api::attribution::ToolKind;

    struct StubTool {
        name: &'static str,
    }

    impl Attributable for StubTool {
        fn role(&self) -> Role {
            Role::Tool(ToolKind::Plugin)
        }
        fn alias(&self) -> &str {
            self.name
        }
    }

    #[async_trait]
    impl Tool for StubTool {
        fn name(&self) -> &str {
            self.name
        }
        fn description(&self) -> &str {
            "stub"
        }
        fn parameters_schema(&self) -> serde_json::Value {
            serde_json::json!({})
        }
        async fn execute(&self, _args: serde_json::Value) -> anyhow::Result<ToolResult> {
            Ok(ToolResult {
                success: true,
                output: "ran".into(),
                error: None,
            })
        }
    }

    #[tokio::test]
    async fn test_trust_aware_tool_declared_read_only_wins_over_name() {
        // Name looks destructive ("delete_"), but declared safety is read-only.
        let wrapped = TrustAwareTool::new(
            Box::new(StubTool { name: "delete_stub" }),
            Arc::new(TrustConfig::default()),
            ToolSafety::ReadOnly,
        );
        let result = <TrustAwareTool as Tool>::execute(&wrapped, serde_json::json!({}))
            .await
            .unwrap();
        assert!(
            result.success,
            "declared read-only must auto-execute: {:?}",
            result.error
        );
    }

    #[tokio::test]
    async fn test_trust_aware_tool_declared_destructive_requires_approval() {
        // Innocent name, declared destructive → must not execute under default config.
        let wrapped = TrustAwareTool::new(
            Box::new(StubTool { name: "get_stub" }),
            Arc::new(TrustConfig::default()),
            ToolSafety::Destructive,
        );
        let result = <TrustAwareTool as Tool>::execute(&wrapped, serde_json::json!({}))
            .await
            .unwrap();
        assert!(!result.success);
        assert!(result.error.unwrap().contains("destructive"));
    }
}
