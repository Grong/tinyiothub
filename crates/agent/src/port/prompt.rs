//! System prompt 构建器 — vendored 自 zeroclaw-runtime `agent/prompt.rs`
//! （PromptSection trait / PromptContext / SystemPromptBuilder，连接规则一致：
//! 逐 section `trim_end` 后加 `"\n\n"`，空 section 跳过）。
//!
//! PromptContext 只保留 9 个 vendored section 实际用到的字段；zeroclaw 原字段
//! （skills / skills_prompt_mode / identity_config / dispatcher_instructions /
//! sends_native_tool_specs / autonomy_level）未 port，理由见 prompt_sections.rs
//! 模块注释。

use std::path::Path;

pub trait PromptSection: Send + Sync {
    fn name(&self) -> &str;
    fn build(&self, ctx: &PromptContext<'_>) -> anyhow::Result<String>;
}

pub struct PromptContext<'a> {
    pub workspace_dir: &'a Path,
    pub agent_workspace_dir: &'a Path,
    pub model_name: &'a str,
    pub tool_specs: Vec<crate::port::tool::ToolSpec>,
    pub security_summary: Option<String>,
}

pub struct SystemPromptBuilder {
    sections: Vec<Box<dyn PromptSection>>,
}

impl SystemPromptBuilder {
    pub fn with_defaults() -> Self {
        Self {
            sections: vec![
                Box::new(crate::port::prompt_sections::DateTimeSection),
                Box::new(crate::port::prompt_sections::IdentitySection),
                Box::new(crate::port::prompt_sections::ToolHonestySection),
                Box::new(crate::port::prompt_sections::ToolsSection),
                Box::new(crate::port::prompt_sections::SafetySection),
                Box::new(crate::port::prompt_sections::SkillsSection),
                Box::new(crate::port::prompt_sections::WorkspaceSection),
                Box::new(crate::port::prompt_sections::RuntimeSection),
                Box::new(crate::port::prompt_sections::ChannelMediaSection),
            ],
        }
    }

    pub fn add_section(mut self, section: Box<dyn PromptSection>) -> Self {
        self.sections.push(section);
        self
    }

    pub fn build(&self, ctx: &PromptContext<'_>) -> anyhow::Result<String> {
        let mut output = String::new();
        for section in &self.sections {
            let part = section.build(ctx)?;
            if part.trim().is_empty() {
                continue;
            }
            output.push_str(part.trim_end());
            output.push_str("\n\n");
        }
        Ok(output)
    }
}
