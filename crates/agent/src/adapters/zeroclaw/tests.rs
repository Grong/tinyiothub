//! zeroclaw 适配器桥接测试。

use crate::adapters::zeroclaw::tools::PortToolAsZeroclaw;
use crate::port::attribution::{Attributable, Role, ToolKind};
use crate::port::tool::{Tool, ToolResult};

struct DummyTool;

impl Attributable for DummyTool {
    fn role(&self) -> Role {
        Role::Tool(ToolKind::Plugin)
    }
    fn alias(&self) -> &str {
        "dummy"
    }
}

#[async_trait::async_trait]
impl Tool for DummyTool {
    fn name(&self) -> &str {
        "dummy_tool"
    }
    fn description(&self) -> &str {
        "A dummy tool for bridge tests"
    }
    fn parameters_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": { "x": { "type": "string" } },
        })
    }
    async fn execute(&self, _args: serde_json::Value) -> anyhow::Result<ToolResult> {
        Ok(ToolResult {
            success: true,
            output: "ok".into(),
            error: None,
        })
    }
}

#[test]
fn port_tool_bridge_preserves_identity() {
    let bridge = PortToolAsZeroclaw(Box::new(DummyTool));

    // zeroclaw 侧视图
    use zeroclaw::tools::Tool as ZcTool;
    assert_eq!(bridge.name(), "dummy_tool");
    assert_eq!(bridge.description(), "A dummy tool for bridge tests");
    assert_eq!(
        bridge.parameters_schema(),
        serde_json::json!({"type":"object","properties":{"x":{"type":"string"}}})
    );
    let spec = bridge.spec();
    assert_eq!(spec.name, "dummy_tool");
    assert_eq!(spec.description, "A dummy tool for bridge tests");

    // port 侧视图不变
    assert_eq!(bridge.0.name(), "dummy_tool");
}

#[test]
fn kind_conversion_round_trips_port_side() {
    use crate::adapters::zeroclaw::tools::{port_memory_kind, port_tool_kind, zc_memory_kind, zc_role, zc_tool_kind};
    // port → zc → port 恒等（变体名/顺序两侧一致是宏的编译期前提）
    for kind in [ToolKind::Plugin, ToolKind::Search, ToolKind::Shell] {
        assert_eq!(port_tool_kind(zc_tool_kind(kind)), kind);
    }
    let mk = crate::port::attribution::MemoryKind::None;
    assert_eq!(port_memory_kind(zc_memory_kind(mk)), mk);

    // zc_role 冒烟：结构映射到 zeroclaw 侧对应变体
    match zc_role(&Role::Tool(ToolKind::Search)) {
        zeroclaw_api::attribution::Role::Tool(zeroclaw_api::attribution::ToolKind::Search) => {}
        other => panic!("unexpected bridged role: {other:?}"),
    }
}
