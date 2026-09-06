//! 自有 agent 引擎接口面（反腐败层）。形状 vendored 自 zeroclaw-api ——
//! 它们是合理的库级设计，且既有工具实现/ScriptedProvider 已长在该形状上。
pub mod attribution;
pub mod events;
pub mod memory;
pub mod observer;
pub mod outcome;
pub mod prompt;
pub mod prompt_sections;
pub mod provider;
pub mod runtime;
pub mod tool;

#[cfg(test)]
mod tests {
    use super::tool::{Tool, ToolResult};
    use super::attribution::{Attributable, Role, ToolKind};

    struct Dummy;
    impl Attributable for Dummy {
        fn role(&self) -> Role { Role::Tool(ToolKind::Plugin) }
        fn alias(&self) -> &str { "dummy" }
    }
    #[async_trait::async_trait]
    impl Tool for Dummy {
        fn name(&self) -> &str { "dummy" }
        fn description(&self) -> &str { "d" }
        fn parameters_schema(&self) -> serde_json::Value { serde_json::json!({"type":"object"}) }
        async fn execute(&self, _args: serde_json::Value) -> anyhow::Result<ToolResult> {
            Ok(ToolResult { success: true, output: "ok".into(), error: None })
        }
    }

    #[test]
    fn tool_spec_default_method() {
        let t = Dummy;
        let spec = t.spec();
        assert_eq!(spec.name, "dummy");
        assert_eq!(spec.description, "d");
    }
}
