use serde::{Deserialize, Serialize};

/// Boilerplate-collapsing macro: pair a concrete `Tool` impl with a
/// matching `Attributable` impl that surfaces the supplied `ToolKind`
/// and uses the tool's `name()` as its alias.
///
/// Invoke once per `Tool` struct, in the same module as the struct:
///
/// ```ignore
/// crate::port::tool_attribution!(ShellTool, crate::port::attribution::ToolKind::Shell);
/// ```
#[macro_export]
macro_rules! tool_attribution {
    ($ty:ty, $kind:expr) => {
        impl $crate::port::attribution::Attributable for $ty {
            fn role(&self) -> $crate::port::attribution::Role {
                $crate::port::attribution::Role::Tool($kind)
            }
            fn alias(&self) -> &str {
                <Self as $crate::port::tool::Tool>::name(self)
            }
        }
    };
}

/// Bulk-impl `Attributable` for one or more `Tool` mock types in a
/// test module. Every type gets `Role::Tool(ToolKind::Plugin)` and uses
/// the mock's own `name()` as the alias — sufficient for test
/// scaffolding where individual kinds don't matter.
///
/// ```ignore
/// tinyiothub_agent::mock_tool_attribution!(CountingTool, FailingTool);
/// ```
#[macro_export]
macro_rules! mock_tool_attribution {
    ($($ty:ty),+ $(,)?) => {
        $(
            $crate::tool_attribution!($ty, $crate::port::attribution::ToolKind::Plugin);
        )+
    };
}

/// Result of a tool execution
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolResult {
    pub success: bool,
    pub output: String,
    pub error: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub attachments: Vec<tinyiothub_core::media::MediaAttachment>,
}

/// Description of a tool for the LLM
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolSpec {
    pub name: String,
    pub description: String,
    pub parameters: serde_json::Value,
}

/// Core tool trait — implement for any capability.
///
/// Every `Tool` is `Attributable`: log emissions and audit traces from
/// a tool call carry the same `<kind>.<alias>` composite the rest of
/// the runtime uses for channels, providers, and memory. The supertrait
/// bound makes `&dyn Tool` coerce to `&dyn Attributable` automatically,
/// so dispatch-site logging can attribute without knowing the concrete
/// tool type.
#[async_trait::async_trait]
pub trait Tool: Send + Sync + crate::port::attribution::Attributable {
    /// Tool name (used in LLM function calling)
    fn name(&self) -> &str;

    /// Human-readable description
    fn description(&self) -> &str;

    /// JSON schema for parameters
    fn parameters_schema(&self) -> serde_json::Value;

    /// Execute the tool with given arguments
    async fn execute(&self, args: serde_json::Value) -> anyhow::Result<ToolResult>;

    /// Whether this tool is advertised to the model. Hidden tools stay
    /// registered (approval paths can resolve them) but never enter the
    /// model's tool list or catalog. E2: legacy device tools are hidden.
    fn exposed(&self) -> bool {
        true
    }

    /// The single declaration source for exposure, prompt, policy, redaction,
    /// and display. The default derives safe values from the name, description,
    /// and argument schema; tools override only the fields they declare
    /// differently. Adding a tool therefore never requires editing core or a
    /// frontend event allowlist.
    fn descriptor(&self) -> tinyiothub_skills::registry::ToolDescriptor {
        let mut descriptor = tinyiothub_skills::registry::ToolDescriptor::declared(
            self.name(),
            self.description(),
            self.parameters_schema(),
        );
        descriptor.exposed = self.exposed();
        descriptor
    }

    /// Get the full spec for LLM registration. Dispatch identity always comes
    /// from `Tool::name()`/`parameters_schema()`; the descriptor contributes
    /// only metadata (description). A descriptor with a mismatched name can
    /// therefore never advertise an undispatchable tool.
    fn spec(&self) -> ToolSpec {
        let description = self.descriptor().llm_description();
        ToolSpec {
            name: self.name().to_string(),
            description,
            parameters: self.parameters_schema(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tinyiothub_skills::registry::{ToolEffectKind, ToolRegistry};

    struct DeclaredTestTool;

    impl crate::port::attribution::Attributable for DeclaredTestTool {
        fn role(&self) -> crate::port::attribution::Role {
            crate::port::attribution::Role::Tool(crate::port::attribution::ToolKind::Plugin)
        }
        fn alias(&self) -> &str {
            "declare_test"
        }
    }

    #[async_trait::async_trait]
    impl Tool for DeclaredTestTool {
        fn name(&self) -> &str {
            "declare_test"
        }
        fn description(&self) -> &str {
            "A declared test tool"
        }
        fn parameters_schema(&self) -> serde_json::Value {
            serde_json::json!({"type": "object", "properties": {"value": {"type": "string"}}})
        }
        fn descriptor(&self) -> tinyiothub_skills::registry::ToolDescriptor {
            let mut descriptor = tinyiothub_skills::registry::ToolDescriptor::declared(
                self.name(),
                self.description(),
                self.parameters_schema(),
            );
            descriptor.prompt_summary = "Prefer this for declaration tests.".into();
            descriptor.required_capabilities = vec![tinyiothub_skills::registry::Capability::Vision];
            descriptor.presentation.label = "声明测试".into();
            descriptor
        }
        async fn execute(&self, _args: serde_json::Value) -> anyhow::Result<ToolResult> {
            Ok(ToolResult {
                success: true,
                output: "ok".into(),
                error: None,
                attachments: vec![],
            })
        }
    }

    struct MismatchedDescriptorTool;

    impl crate::port::attribution::Attributable for MismatchedDescriptorTool {
        fn role(&self) -> crate::port::attribution::Role {
            crate::port::attribution::Role::Tool(crate::port::attribution::ToolKind::Plugin)
        }
        fn alias(&self) -> &str {
            "dispatch_name"
        }
    }

    #[async_trait::async_trait]
    impl Tool for MismatchedDescriptorTool {
        fn name(&self) -> &str {
            "dispatch_name"
        }
        fn description(&self) -> &str {
            "Dispatchable tool"
        }
        fn parameters_schema(&self) -> serde_json::Value {
            serde_json::json!({"type": "object", "properties": {"dispatchArg": {"type": "string"}}})
        }
        fn descriptor(&self) -> tinyiothub_skills::registry::ToolDescriptor {
            tinyiothub_skills::registry::ToolDescriptor::declared(
                "different_alias",
                "Descriptor metadata",
                serde_json::json!({"type": "object", "properties": {"other": {"type": "string"}}}),
            )
        }
        async fn execute(&self, _args: serde_json::Value) -> anyhow::Result<ToolResult> {
            Ok(ToolResult {
                success: true,
                output: "ok".into(),
                error: None,
                attachments: vec![],
            })
        }
    }

    #[test]
    fn tools_descriptor_mismatched_name_never_changes_dispatch_identity() {
        let tool = MismatchedDescriptorTool;
        let spec = tool.spec();
        assert_eq!(spec.name, "dispatch_name", "LLM must see the dispatchable name");
        assert_eq!(
            spec.parameters,
            serde_json::json!({"type": "object", "properties": {"dispatchArg": {"type": "string"}}})
        );
        assert_eq!(
            tool.descriptor().name,
            "different_alias",
            "metadata alias is not dispatch identity"
        );
    }

    #[test]
    fn tools_descriptor_drives_spec_and_safe_defaults() {
        let tool = DeclaredTestTool;
        let descriptor = tool.descriptor();
        assert_eq!(descriptor.effect_kind, ToolEffectKind::Write);
        assert!(
            descriptor
                .missing_capabilities(&[])
                .contains(&tinyiothub_skills::registry::Capability::Vision)
        );
        assert_eq!(descriptor.presentation.label, "声明测试");

        let spec = tool.spec();
        assert_eq!(spec.name, "declare_test");
        assert!(
            spec.description.contains("Prefer this for declaration tests."),
            "prompt summary must be part of the LLM spec"
        );

        // A declaration alone is enough to expose the tool through a registry.
        let registry = tinyiothub_skills::registry::DeclaredToolRegistry::new(vec![descriptor]);
        let llm = registry.generate_llm_schema_for(&[tinyiothub_skills::registry::Capability::Vision], None);
        assert_eq!(llm["tools"][0]["name"], "declare_test");
    }
}
