//! ToolExec 接缝：统一工具执行外壳（spec §2.5）。
//!
//! 每个解析出的工具被 [`SeamTool`] 包装，`execute` 路由到接缝；默认实现
//! 直接调用内层工具。替换实现可以审计/包装，但必须恰好调用内层一次。

use std::sync::Arc;

use crate::port::attribution::{Attributable, Role};
use crate::port::tool::{Tool, ToolResult};

use super::Seam;

pub struct ToolInvocation {
    pub workspace_id: String,
    pub tool: Arc<dyn Tool>,
    pub args: serde_json::Value,
}

#[async_trait::async_trait]
pub trait ToolExecSeam: Seam {
    async fn execute(&self, inv: ToolInvocation) -> anyhow::Result<ToolResult>;

    fn next(&self) -> Option<&dyn ToolExecSeam> {
        None
    }
}

pub struct DefaultToolExec;

#[async_trait::async_trait]
impl ToolExecSeam for DefaultToolExec {
    async fn execute(&self, inv: ToolInvocation) -> anyhow::Result<ToolResult> {
        inv.tool.execute(inv.args).await
    }
}

/// 把任一工具包成接缝入口。内层工具保持原有 policy/confirm/arbitration 语义。
pub struct SeamTool {
    inner: Arc<dyn Tool>,
    seam: Arc<dyn ToolExecSeam>,
    workspace_id: String,
}

impl SeamTool {
    pub fn new(inner: Box<dyn Tool>, seam: Arc<dyn ToolExecSeam>, workspace_id: impl Into<String>) -> Self {
        Self {
            inner: Arc::from(inner),
            seam,
            workspace_id: workspace_id.into(),
        }
    }
}

impl Attributable for SeamTool {
    fn role(&self) -> Role {
        self.inner.role()
    }
    fn alias(&self) -> &str {
        self.inner.alias()
    }
}

#[async_trait::async_trait]
impl Tool for SeamTool {
    fn name(&self) -> &str {
        self.inner.name()
    }
    fn description(&self) -> &str {
        self.inner.description()
    }
    fn parameters_schema(&self) -> serde_json::Value {
        self.inner.parameters_schema()
    }
    fn exposed(&self) -> bool {
        self.inner.exposed()
    }
    async fn execute(&self, args: serde_json::Value) -> anyhow::Result<ToolResult> {
        self.seam
            .execute(ToolInvocation {
                workspace_id: self.workspace_id.clone(),
                tool: Arc::clone(&self.inner),
                args,
            })
            .await
    }
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicUsize, Ordering};

    use super::*;
    use crate::port::attribution::ToolKind;
    use crate::port::tool::ToolResult;

    struct EchoTool;

    impl Attributable for EchoTool {
        fn role(&self) -> Role {
            Role::Tool(ToolKind::Plugin)
        }
        fn alias(&self) -> &str {
            "echo"
        }
    }

    #[async_trait::async_trait]
    impl Tool for EchoTool {
        fn name(&self) -> &str {
            "echo"
        }
        fn description(&self) -> &str {
            "echo"
        }
        fn parameters_schema(&self) -> serde_json::Value {
            serde_json::json!({})
        }
        async fn execute(&self, args: serde_json::Value) -> anyhow::Result<ToolResult> {
            Ok(ToolResult {
                success: true,
                output: args.to_string(),
                error: None,
            })
        }
    }

    /// 替换证明：审计后原样委托，内层恰好调用一次。
    struct AuditToolExec {
        inner: DefaultToolExec,
        calls: Arc<AtomicUsize>,
    }

    #[async_trait::async_trait]
    impl ToolExecSeam for AuditToolExec {
        async fn execute(&self, inv: ToolInvocation) -> anyhow::Result<ToolResult> {
            self.calls.fetch_add(1, Ordering::SeqCst);
            self.inner.execute(inv).await
        }
        fn next(&self) -> Option<&dyn ToolExecSeam> {
            Some(&self.inner)
        }
    }

    #[tokio::test]
    async fn seam_tool_routes_through_replacement_and_delegates() {
        let calls = Arc::new(AtomicUsize::new(0));
        let seam = Arc::new(AuditToolExec {
            inner: DefaultToolExec,
            calls: Arc::clone(&calls),
        });
        let tool = SeamTool::new(Box::new(EchoTool), seam, "ws");
        let out = tool.execute(serde_json::json!({"n": 1})).await.unwrap();
        assert!(out.success);
        assert_eq!(out.output, "{\"n\":1}");
        assert_eq!(calls.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn default_exec_calls_inner_once() {
        let seam = Arc::new(DefaultToolExec);
        let tool = SeamTool::new(Box::new(EchoTool), seam, "ws");
        let out = tool.execute(serde_json::json!({"n": 2})).await.unwrap();
        assert_eq!(out.output, "{\"n\":2}");
    }
}
