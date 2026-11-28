//! ToolRegistry — 内建工具 provider 注册点 + 工具加载/解析（Task 14）。
//!
//! 框架住本 crate；数据工具实现（thing 工具、canvas 等）住组合层，启动时经
//! [`ToolRegistry::register_provider`] 闭包注入 —— 闭包捕获持久化句柄，
//! 本 crate 只面向 `Box<dyn Tool>`。外部（MCP）工具经
//! [`ToolRegistry::set_external_tool_factory`] 按需派生，保持动态注册可见。

use parking_lot::RwLock;
use std::sync::Arc;

use tinyiothub_agent::port::tool::Tool;
use tinyiothub_core::heartbeat::TrustConfig;
use tinyiothub_skills::registry::{
    Capability, DeclaredToolRegistry, PresentationDescriptor, ToolDescriptor, ToolEffectKind, ToolRegistry as _,
    ToolRegistryError, verify_revision_policy,
};
use tinyiothub_skills::trust::ToolSafety;

use tinyiothub_agent::config::AgentRuntimeConfig;

use super::catalog;
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

fn safety_rank(safety: ToolSafety) -> u8 {
    match safety {
        ToolSafety::ReadOnly => 0,
        ToolSafety::Write => 1,
        ToolSafety::Destructive => 2,
    }
}

fn max_safety(left: ToolSafety, right: ToolSafety) -> ToolSafety {
    if safety_rank(left) >= safety_rank(right) {
        left
    } else {
        right
    }
}

fn effect_kind_rank(kind: ToolEffectKind) -> u8 {
    match kind {
        ToolEffectKind::Read => 0,
        ToolEffectKind::Write => 1,
        ToolEffectKind::External => 2,
        ToolEffectKind::Destructive => 3,
    }
}

fn max_effect_kind(declared: ToolEffectKind, safety: ToolSafety) -> ToolEffectKind {
    let inferred = ToolEffectKind::from_safety(safety);
    if effect_kind_rank(declared) >= effect_kind_rank(inferred) {
        declared
    } else {
        inferred
    }
}

fn safety_from_effect_kind(kind: ToolEffectKind) -> ToolSafety {
    match kind {
        ToolEffectKind::Read => ToolSafety::ReadOnly,
        ToolEffectKind::Write | ToolEffectKind::External => ToolSafety::Write,
        ToolEffectKind::Destructive => ToolSafety::Destructive,
    }
}

/// Adapt a runtime tool plus its registry-declared safety into the single
/// `ToolDescriptor` used by exposure, policy, redaction, and display.
///
/// The tool's own declaration is the source of truth. The registry-declared
/// safety only strengthens it (declared destructive cannot be downgraded), and
/// existing display labels/groups are applied when the tool did not declare a
/// presentation override.
pub fn descriptor_for(tool: &dyn Tool, safety: ToolSafety) -> ToolDescriptor {
    let mut descriptor = tool.descriptor();
    // Precedence is monotonic: neither declaration nor registry safety may
    // downgrade the other. A tool that declares Destructive/External stays at
    // least that strong; a registry-declared Destructive can never be masked by
    // an optimistic descriptor.
    let declared_safety = safety_from_effect_kind(descriptor.effect_kind);
    descriptor.safety = max_safety(max_safety(descriptor.safety, safety), declared_safety);
    descriptor.effect_kind = max_effect_kind(descriptor.effect_kind, safety);
    if matches!(descriptor.safety, ToolSafety::Destructive) {
        descriptor.approval_policy.mandatory = true;
        if descriptor.approval_policy.reason.is_none() {
            descriptor.approval_policy.reason = Some("destructive tool".to_string());
        }
        descriptor.requires_approval = true;
    }

    let danger = matches!(descriptor.safety, ToolSafety::Destructive) || catalog::name_infers_destructive(tool.name());
    let inferred = PresentationDescriptor::inferred(tool.name(), danger);
    let mut presentation = descriptor.presentation.clone();
    // An explicit label/group (different from the safe fallback) wins.
    if presentation.label == inferred.label {
        presentation.label = catalog::tool_label(tool.name()).to_string();
    }
    if presentation.group == inferred.group {
        let (group, group_label) = catalog::tool_group(tool.name());
        presentation.group = group.to_string();
        presentation.group_label = group_label.to_string();
    }
    presentation.danger = danger;
    presentation.enabled = !danger;
    presentation.view_key = Some(catalog::tool_view_key(tool.name()).to_string());
    descriptor.presentation = presentation;
    descriptor
}

impl ToolRegistry {
    /// 注册内建工具 provider（组合层启动时调用；重复注册会重复产出工具，
    /// 调用方保证只注册一次）。
    pub fn register_provider(&self, provider: ToolProvider) {
        // T14：parking_lot 锁无中毒，expect 根除。
        self.inner.write().providers.push(provider);
    }

    /// 注册外部工具 registry 工厂（组合层启动时调用）。
    pub fn set_external_tool_factory(&self, factory: ExternalToolFactory) {
        self.inner.write().external_factory = Some(factory);
    }

    fn providers(&self) -> Vec<ToolProvider> {
        self.inner.read().providers.clone()
    }

    pub(crate) fn external_registry(&self) -> Option<Arc<dyn ExternalToolRegistry>> {
        let factory = self.inner.read().external_factory.clone()?;
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

    /// Build the single descriptor source for every loaded tool (built-in and
    /// external). All exposure surfaces derive from this list.
    pub async fn descriptors(&self, workspace_id: &str, runtime: &ToolRuntimeContext) -> Vec<ToolDescriptor> {
        self.load_with_safety(workspace_id, runtime)
            .await
            .into_iter()
            .map(|(tool, safety)| descriptor_for(tool.as_ref(), safety))
            .collect()
    }

    /// LLM tool subset generated from declarations for the capabilities the
    /// composition root resolved. Hidden aliases never appear.
    pub async fn llm_tool_schema(
        &self,
        workspace_id: &str,
        runtime: &ToolRuntimeContext,
        available: &[Capability],
    ) -> serde_json::Value {
        DeclaredToolRegistry::new(self.descriptors(workspace_id, runtime).await)
            .generate_llm_schema_for(available, None)
    }

    /// Redacted public argument summary. Raw argument JSON is internal; this is
    /// the only argument representation exposed by DTO/SSE metadata.
    pub async fn public_arguments_summary(
        &self,
        workspace_id: &str,
        runtime: &ToolRuntimeContext,
        name: &str,
        arguments: &serde_json::Value,
    ) -> Result<String, ToolRegistryError> {
        DeclaredToolRegistry::new(self.descriptors(workspace_id, runtime).await).redacted_arguments(name, arguments)
    }

    /// Effective approval policy for a tool with optional Workflow tightening.
    /// Workflow policy can only tighten the declared mandatory policy.
    pub async fn effective_approval_for(
        &self,
        workspace_id: &str,
        runtime: &ToolRuntimeContext,
        name: &str,
        workflow: Option<&tinyiothub_skills::registry::ApprovalPolicy>,
    ) -> Result<tinyiothub_skills::registry::ApprovalPolicy, ToolRegistryError> {
        DeclaredToolRegistry::new(self.descriptors(workspace_id, runtime).await).effective_approval(name, workflow)
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
            .filter(|(tool, safety)| {
                // Production policy gate: a Workflow revision may tighten the
                // descriptor, but the current effective policy must equal the
                // pinned revision policy. Any mismatch fails closed by not
                // exposing the tool for dispatch.
                let Some(policy) = &runtime.tool_policy else {
                    return true;
                };
                let descriptor = descriptor_for(tool.as_ref(), *safety);
                let effective = descriptor.effective_approval(policy.workflow_approval_policy.as_ref());
                let Some(pinned) = policy.pinned_tool_policies.get(tool.name()) else {
                    return true;
                };
                if let Err(error) = verify_revision_policy(pinned, &effective) {
                    tracing::error!(
                        workspace_id,
                        tool = %tool.name(),
                        %error,
                        "tool policy revision mismatch — tool not exposed"
                    );
                    return false;
                }
                true
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
    use tinyiothub_agent::port::attribution::{Attributable, Role, ToolKind};
    use tinyiothub_agent::port::tool::ToolResult;

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
                attachments: vec![],
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

    struct DescriptorStub {
        descriptor: ToolDescriptor,
    }

    impl Attributable for DescriptorStub {
        fn role(&self) -> Role {
            Role::Tool(ToolKind::Plugin)
        }
        fn alias(&self) -> &str {
            "infosync"
        }
    }

    #[async_trait]
    impl Tool for DescriptorStub {
        fn name(&self) -> &str {
            "infosync"
        }
        fn description(&self) -> &str {
            "declared tool"
        }
        fn parameters_schema(&self) -> serde_json::Value {
            serde_json::json!({"type": "object"})
        }
        fn descriptor(&self) -> ToolDescriptor {
            self.descriptor.clone()
        }
        async fn execute(&self, _args: serde_json::Value) -> anyhow::Result<ToolResult> {
            Ok(ToolResult {
                success: true,
                output: "ran".into(),
                error: None,
                attachments: vec![],
            })
        }
    }

    #[test]
    fn registry_safety_and_effect_kind_never_downgrade_declarations() {
        use tinyiothub_skills::registry::ToolEffectKind;

        let mut declared_external = ToolDescriptor::declared("infosync", "sync", serde_json::json!({}));
        declared_external.effect_kind = ToolEffectKind::External;
        declared_external.safety = ToolSafety::Write;
        let descriptor = descriptor_for(
            &DescriptorStub {
                descriptor: declared_external,
            },
            ToolSafety::ReadOnly,
        );
        assert_eq!(
            descriptor.effect_kind,
            ToolEffectKind::External,
            "registry read-only must not downgrade a declared external effect"
        );
        assert_eq!(descriptor.safety, ToolSafety::Write);

        let mut declared_destructive = ToolDescriptor::declared("infosync", "sync", serde_json::json!({}));
        declared_destructive.effect_kind = ToolEffectKind::Destructive;
        declared_destructive.safety = ToolSafety::ReadOnly;
        let descriptor = descriptor_for(
            &DescriptorStub {
                descriptor: declared_destructive,
            },
            ToolSafety::ReadOnly,
        );
        assert_eq!(descriptor.effect_kind, ToolEffectKind::Destructive);
        assert!(descriptor.approval_policy.mandatory);

        let declared_read = ToolDescriptor::declared("infosync", "sync", serde_json::json!({}));
        let descriptor = descriptor_for(
            &DescriptorStub {
                descriptor: declared_read,
            },
            ToolSafety::Destructive,
        );
        assert_eq!(
            descriptor.effect_kind,
            ToolEffectKind::Destructive,
            "registry destructive must not be downgraded by the declaration"
        );
        assert_eq!(descriptor.safety, ToolSafety::Destructive);
        assert!(descriptor.approval_policy.mandatory);
    }

    #[tokio::test]
    async fn resolve_tools_fails_closed_when_pinned_policy_mismatches() {
        use tinyiothub_skills::registry::ApprovalPolicy;

        use crate::domains::agent::tools::context::ToolPolicyContext;

        let registry = ToolRegistry::default();
        registry.register_provider(Arc::new(|_ws, _rt| {
            vec![(
                Box::new(NamedStubTool { name: "stub_tool" }) as Box<dyn Tool>,
                ToolSafety::Write,
            )]
        }));

        let pinned_mismatch = ToolPolicyContext {
            workflow_approval_policy: None,
            pinned_tool_policies: [(
                "stub_tool".to_string(),
                ApprovalPolicy {
                    mandatory: true,
                    timeout_secs: 60,
                    reason: Some("pinned".into()),
                },
            )]
            .into_iter()
            .collect(),
        };
        let runtime = ToolRuntimeContext {
            tool_policy: Some(pinned_mismatch),
            ..ToolRuntimeContext::default()
        };
        let tools = registry
            .resolve_tools_for_agent(&AgentRuntimeConfig::default(), "ws", None, &runtime)
            .await;
        assert!(tools.is_empty(), "a revision policy mismatch must fail closed");

        let pinned_match = ToolPolicyContext {
            workflow_approval_policy: None,
            pinned_tool_policies: [("stub_tool".to_string(), ApprovalPolicy::none())]
                .into_iter()
                .collect(),
        };
        let runtime = ToolRuntimeContext {
            tool_policy: Some(pinned_match),
            ..ToolRuntimeContext::default()
        };
        let tools = registry
            .resolve_tools_for_agent(&AgentRuntimeConfig::default(), "ws", None, &runtime)
            .await;
        assert_eq!(tools.len(), 1, "matching pinned policy resolves normally");
    }
}
