//! ToolRegistry — 内建工具 provider 注册点 + 工具加载/解析（Task 14）。
//!
//! 框架住本 crate；数据工具实现（thing 工具、canvas 等）住组合层，启动时经
//! [`ToolRegistry::register_provider`] 闭包注入 —— 闭包捕获持久化句柄，
//! 本 crate 只面向 `Box<dyn Tool>`。外部（MCP）工具经
//! [`ToolRegistry::set_external_tool_factory`] 按需派生，保持动态注册可见。

use std::sync::{Arc, RwLock};

use tinyiothub_core::heartbeat::TrustConfig;
use tinyiothub_skills::trust::ToolSafety;
use zeroclaw::tools::Tool;

use crate::config::AgentRuntimeConfig;

use super::context::ToolRuntimeContext;
use super::external::{ExternalToolRegistry, IoTToolAdapter};
use super::trust::TrustAwareTool;

/// 内建工具 provider：给定 workspace 与运行时句柄，产出工具及其安全分级。
/// 组合层注册（闭包捕获 db pool / device cache / pending-action store 等
/// 数据句柄 —— D2：这些类型不进本 crate）。
pub type ToolProvider = Arc<dyn Fn(&str, &ToolRuntimeContext) -> Vec<(Box<dyn Tool>, ToolSafety)> + Send + Sync>;

/// 外部工具 registry 工厂：按需派生（G3 —— 单一事实源在组合层，本 crate
/// 不持有第二个注册静态）。
pub type ExternalToolFactory = Arc<dyn Fn() -> Option<Arc<dyn ExternalToolRegistry>> + Send + Sync>;

#[derive(Default)]
struct RegistryInner {
    providers: Vec<ToolProvider>,
    external_factory: Option<ExternalToolFactory>,
}

/// 工具注册表 — Clone 句柄共享同一内部状态（AgentPool 持有一份，组合层经
/// `pool.tool_registry()` 注册）。
#[derive(Clone, Default)]
pub struct ToolRegistry {
    inner: Arc<RwLock<RegistryInner>>,
}

impl ToolRegistry {
    /// 注册内建工具 provider（组合层启动时调用；重复注册会重复产出工具，
    /// 调用方保证只注册一次）。
    pub fn register_provider(&self, provider: ToolProvider) {
        self.inner
            .write()
            .expect("tool registry lock poisoned")
            .providers
            .push(provider);
    }

    /// 注册外部工具 registry 工厂（组合层启动时调用）。
    pub fn set_external_tool_factory(&self, factory: ExternalToolFactory) {
        self.inner
            .write()
            .expect("tool registry lock poisoned")
            .external_factory = Some(factory);
    }

    fn providers(&self) -> Vec<ToolProvider> {
        self.inner
            .read()
            .expect("tool registry lock poisoned")
            .providers
            .clone()
    }

    pub(crate) fn external_registry(&self) -> Option<Arc<dyn ExternalToolRegistry>> {
        let factory = self
            .inner
            .read()
            .expect("tool registry lock poisoned")
            .external_factory
            .clone()?;
        factory()
    }

    /// Load all tools: registered built-in providers first, then external
    /// (MCP) handlers — built-in names win on collision (T7).
    pub async fn load_all_tools(&self, workspace_id: &str, runtime: &ToolRuntimeContext) -> Vec<Box<dyn Tool>> {
        self.load_with_safety(workspace_id, runtime)
            .await
            .into_iter()
            .map(|(tool, _)| tool)
            .collect()
    }

    /// Load all tools together with their (declared or name-inferred) safety.
    /// External adapters carry the handler's declared safety; built-in tools
    /// carry the provider's classification.
    pub async fn load_with_safety(
        &self,
        workspace_id: &str,
        runtime: &ToolRuntimeContext,
    ) -> Vec<(Box<dyn Tool>, ToolSafety)> {
        let mut tools: Vec<(Box<dyn Tool>, ToolSafety)> = Vec::new();
        let mut builtin_names: std::collections::HashSet<String> = std::collections::HashSet::new();

        for provider in self.providers() {
            for (tool, safety) in provider(workspace_id, runtime) {
                builtin_names.insert(tool.name().to_string());
                tools.push((tool, safety));
            }
        }

        if let Some(registry) = self.external_registry() {
            for meta in registry.list_tools().await {
                if meta.name.trim().is_empty() {
                    continue;
                }
                // Built-in tools take precedence: an external handler with the
                // same name (e.g. a handler renamed to "get_thing") would
                // double-register with a different schema (eng-review T7).
                if builtin_names.contains(&meta.name) {
                    tracing::warn!(tool = %meta.name, "Skipping external handler colliding with built-in tool");
                    continue;
                }
                let name = meta.name.clone();
                let description = meta.description.clone();
                let input_schema = meta.input_schema.clone();
                if let Some(handler) = registry.get_handler(&name).await {
                    let adapter =
                        IoTToolAdapter::new(name, description, input_schema, handler, workspace_id.to_string());
                    let safety = adapter.safety();
                    tools.push((Box::new(adapter), safety));
                }
            }
        }

        tools
    }

    /// Load and filter tools for an agent based on its runtime config.
    /// If `trust_config` is provided, wraps every tool with `TrustAwareTool`
    /// for trust-level enforcement at execution time.
    pub async fn resolve_tools_for_agent(
        &self,
        config: &AgentRuntimeConfig,
        workspace_id: &str,
        trust_config: Option<Arc<TrustConfig>>,
        runtime: &ToolRuntimeContext,
    ) -> Vec<Box<dyn Tool>> {
        let all_tools = self.load_with_safety(workspace_id, runtime).await;
        let filtered: Vec<(Box<dyn Tool>, ToolSafety)> = all_tools
            .into_iter()
            .filter(|(tool, _)| {
                let name = tool.name();
                name == "canvas" || !config.tool_denylist.contains(&name.to_string())
            })
            .collect();

        match trust_config {
            Some(tc) => filtered
                .into_iter()
                .map(|(tool, safety)| {
                    let wrapped: Box<dyn Tool> = Box::new(TrustAwareTool::new(tool, Arc::clone(&tc), safety));
                    wrapped
                })
                .collect(),
            None => filtered.into_iter().map(|(tool, _)| tool).collect(),
        }
    }
}

/// Filter tools by denylist, always keeping the canvas tool.
///
/// The canvas tool (name == "canvas") is exempt from denylist filtering
/// because it is a safe A2UI rendering tool, not an IoT operation.
pub fn filter_by_denylist(tools: Vec<Box<dyn Tool>>, denylist: &[String]) -> Vec<Box<dyn Tool>> {
    if denylist.is_empty() {
        return tools;
    }

    tools
        .into_iter()
        .filter(|tool| {
            let name = tool.name();
            if name == "canvas" {
                return true;
            }
            !denylist.contains(&name.to_string())
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use async_trait::async_trait;
    use zeroclaw::tools::ToolResult;
    use zeroclaw_api::attribution::{Attributable, Role, ToolKind};

    pub(crate) struct NamedStubTool {
        pub name: &'static str,
    }

    impl Attributable for NamedStubTool {
        fn role(&self) -> Role {
            Role::Tool(ToolKind::Plugin)
        }
        fn alias(&self) -> &str {
            self.name
        }
    }

    #[async_trait]
    impl Tool for NamedStubTool {
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

    #[test]
    fn test_filter_by_denylist_empty() {
        let tools: Vec<Box<dyn Tool>> = vec![Box::new(NamedStubTool { name: "canvas" })];
        let result = filter_by_denylist(tools, &[]);
        assert!(!result.is_empty());
    }

    #[test]
    fn test_filter_by_denylist_keeps_canvas() {
        let tools: Vec<Box<dyn Tool>> = vec![
            Box::new(NamedStubTool { name: "canvas" }),
            Box::new(NamedStubTool { name: "delete_thing" }),
        ];
        let result = filter_by_denylist(tools, &["delete_thing".to_string(), "canvas".to_string()]);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].name(), "canvas");
    }

    #[tokio::test]
    async fn test_registry_loads_providers_and_skips_external_collisions() {
        struct StubExternalHandler;

        #[async_trait]
        impl super::super::external::ExternalToolHandler for StubExternalHandler {
            fn name(&self) -> &str {
                "get_thing"
            }
            fn description(&self) -> &str {
                "colliding external handler"
            }
            fn input_schema(&self) -> serde_json::Value {
                serde_json::json!({})
            }
            async fn execute(
                &self,
                _ctx: &super::super::external::ExternalToolContext,
                _args: serde_json::Value,
            ) -> Result<serde_json::Value, String> {
                Ok(serde_json::json!({}))
            }
        }

        struct StubExternalRegistry;

        #[async_trait]
        impl ExternalToolRegistry for StubExternalRegistry {
            async fn list_tools(&self) -> Vec<super::super::external::ExternalToolMeta> {
                vec![
                    super::super::external::ExternalToolMeta {
                        name: "get_thing".into(),
                        description: "collides with built-in".into(),
                        input_schema: serde_json::json!({}),
                    },
                    super::super::external::ExternalToolMeta {
                        name: "alarm_list".into(),
                        description: "external only".into(),
                        input_schema: serde_json::json!({}),
                    },
                ]
            }
            async fn get_handler(&self, _name: &str) -> Option<Arc<dyn super::super::external::ExternalToolHandler>> {
                Some(Arc::new(StubExternalHandler))
            }
        }

        let registry = ToolRegistry::default();
        registry.register_provider(Arc::new(|_ws, _rt| {
            vec![(
                Box::new(NamedStubTool { name: "get_thing" }) as Box<dyn Tool>,
                ToolSafety::ReadOnly,
            )]
        }));
        registry.set_external_tool_factory(Arc::new(|| Some(Arc::new(StubExternalRegistry))));

        let tools = registry.load_all_tools("ws", &ToolRuntimeContext::default()).await;
        let names: Vec<&str> = tools.iter().map(|t| t.name()).collect();
        assert_eq!(names, vec!["get_thing", "alarm_list"]);
        // Built-in wins: the surviving "get_thing" is the provider's stub.
        assert_eq!(tools[0].description(), "stub");
    }
}
