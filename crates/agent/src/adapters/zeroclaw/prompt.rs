//! port SystemPromptBuilder → zeroclaw SystemPromptBuilder 桥。
//!
//! 保真策略（计划 Global Constraints）：默认 9 个 section 不逐段映射——
//! 直接用 zeroclaw 黑盒 `with_defaults()`（我们的 vendored 版本已经
//! `vendored_defaults_match_zeroclaw_black_box` 测试锁定逐字符相等）；
//! 只把 port builder 中的自定义 section（TinyIoTHubSkillsSection 等）追加进去。

use crate::port::prompt::{PromptContext, PromptSection, SystemPromptBuilder};

/// zeroclaw 内置的 9 个默认 section 名（agent/prompt.rs with_defaults 顺序）。
const ZC_DEFAULT_SECTIONS: [&str; 9] = [
    "datetime",
    "identity",
    "tool_honesty",
    "tools",
    "safety",
    "skills",
    "workspace",
    "runtime",
    "channel_media",
];

/// port 自定义 section → zeroclaw PromptSection 桥。
///
/// 渲染时 zeroclaw 传入它的 PromptContext；桥把两侧共有字段映射回 port
/// PromptContext（自定义 section 实际只消费 workspace_dir/agent_workspace_dir）。
struct PortSectionAsZeroclaw(Box<dyn PromptSection>);

impl zeroclaw::agent::prompt::PromptSection for PortSectionAsZeroclaw {
    fn name(&self) -> &str {
        self.0.name()
    }

    fn build(&self, zc_ctx: &zeroclaw::agent::prompt::PromptContext<'_>) -> anyhow::Result<String> {
        let tool_specs = zc_ctx
            .tools
            .iter()
            .map(|t| crate::port::tool::ToolSpec {
                name: t.name().to_string(),
                description: t.description().to_string(),
                parameters: t.parameters_schema(),
            })
            .collect();
        let ctx = PromptContext {
            workspace_dir: zc_ctx.workspace_dir,
            agent_workspace_dir: zc_ctx.agent_workspace_dir,
            model_name: zc_ctx.model_name,
            tool_specs,
            security_summary: zc_ctx.security_summary.clone(),
        };
        self.0.build(&ctx)
    }
}

/// 把 port SystemPromptBuilder 转成 zeroclaw builder：
/// zeroclaw 黑盒默认 9 段 + port 侧追加的自定义段。
pub fn to_zeroclaw_builder(ours: SystemPromptBuilder) -> zeroclaw::agent::prompt::SystemPromptBuilder {
    let mut builder = zeroclaw::agent::prompt::SystemPromptBuilder::with_defaults();
    for section in ours.into_sections() {
        if ZC_DEFAULT_SECTIONS.contains(&section.name()) {
            continue;
        }
        builder = builder.add_section(Box::new(PortSectionAsZeroclaw(section)));
    }
    builder
}
