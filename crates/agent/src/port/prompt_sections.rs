//! System prompt sections — vendored 自 zeroclaw-runtime `agent/prompt.rs`
//! 的 9 个默认 PromptSection 实现。
//!
//! Vendor 策略（对照 zeroclaw v0.8.1-patched `agent/prompt.rs`）：
//! - 输出文本逐字符保真；裁剪的分支在各自 build() 上注释。
//! - `ctx.tools`（`&[Box<dyn Tool>]`）→ `ctx.tool_specs`（`Vec<ToolSpec>`）。
//! - `sends_native_tool_specs` 在 TinyIoTHub 恒为 true
//!   （NativeToolDispatcher::should_send_tool_specs），`ToolsSection` 只 vendor
//!   该分支；`dispatcher_instructions` 未 port（恒 ""）。
//! - `identity_config` 未 port（TinyIoTHub 不用 AIEOS），`IdentitySection`
//!   只 vendor `None` 分支的兜底文本 + personality 渲染；personality 加载器
//!   一并 vendor（`agent/personality.rs`，含 20_000 字符截断）。
//! - `skills` / `skills_prompt_mode` 未 port（TinyIoTHub 经独立 section 注入
//!   skills），`SkillsSection` 等价于 zeroclaw 空 skills 分支（输出空串）。
//! - `autonomy_level` 未 port，按 zeroclaw 默认 `Supervised` vendor
//!   `SafetySection` 文本。

use std::fmt::Write as _;
use std::path::Path;

use anyhow::Result;
use chrono::{Datelike, Local};

use crate::port::prompt::{PromptContext, PromptSection};

/// Maximum characters per personality file before truncation.
/// Vendored 自 zeroclaw-runtime `agent/personality.rs`.
const MAX_FILE_CHARS: usize = 20_000;

/// Well-known personality files loaded from the workspace root.
/// Vendored 自 zeroclaw-runtime `agent/personality.rs`.
const PERSONALITY_FILES: &[&str] = &[
    "SOUL.md",
    "IDENTITY.md",
    "USER.md",
    "AGENTS.md",
    "TOOLS.md",
    "HEARTBEAT.md",
    "BOOTSTRAP.md",
    "MEMORY.md",
];

/// Render all loaded personality files into a prompt fragment.
/// Vendored 自 zeroclaw-runtime `agent/personality.rs` `PersonalityProfile::render`。
fn render_personality(workspace_dir: &Path) -> String {
    let mut out = String::new();
    for filename in PERSONALITY_FILES {
        let Ok(raw) = std::fs::read_to_string(workspace_dir.join(filename)) else {
            continue;
        };
        let trimmed = raw.trim();
        if trimmed.is_empty() {
            continue;
        }
        let (content, truncated) = if trimmed.chars().count() <= MAX_FILE_CHARS {
            (trimmed.to_string(), false)
        } else {
            let cut = trimmed
                .char_indices()
                .nth(MAX_FILE_CHARS)
                .map(|(idx, _)| &trimmed[..idx])
                .unwrap_or(trimmed);
            (cut.to_string(), true)
        };
        let _ = writeln!(out, "### {}\n", filename);
        out.push_str(&content);
        if truncated {
            let _ = writeln!(
                out,
                "\n\n[... truncated at {MAX_FILE_CHARS} chars — use `read` for full file]\n"
            );
        } else {
            out.push_str("\n\n");
        }
    }
    out
}

pub struct IdentitySection;

impl PromptSection for IdentitySection {
    fn name(&self) -> &str {
        "identity"
    }

    fn build(&self, ctx: &PromptContext<'_>) -> Result<String> {
        // zeroclaw 的 AIEOS 分支依赖 identity_config，未 port（恒无 AIEOS）。
        let mut prompt = String::from("## Project Context\n\n");
        prompt.push_str("The following workspace files define your identity, behavior, and context.\n\n");
        prompt.push_str(&render_personality(ctx.agent_workspace_dir));
        Ok(prompt)
    }
}

pub struct ToolHonestySection;

impl PromptSection for ToolHonestySection {
    fn name(&self) -> &str {
        "tool_honesty"
    }

    fn build(&self, ctx: &PromptContext<'_>) -> Result<String> {
        if ctx.tool_specs.is_empty() {
            return Ok(String::new());
        }

        Ok(
            "## CRITICAL: Tool Honesty\n\n\
             - NEVER fabricate, invent, or guess tool results. If a tool returns empty results, say \"No results found.\"\n\
             - If a tool call fails, report the error — never make up data to fill the gap.\n\
             - When unsure whether a tool call succeeded, ask the user rather than guessing."
                .into(),
        )
    }
}

pub struct ToolsSection;

impl PromptSection for ToolsSection {
    fn name(&self) -> &str {
        "tools"
    }

    fn build(&self, ctx: &PromptContext<'_>) -> Result<String> {
        // zeroclaw: `sends_native_tool_specs` 为 true 时返回 dispatcher_instructions
        // （原文：`return Ok(ctx.dispatcher_instructions.to_string())`）。
        // TinyIoTHub 恒走 native 分支且 dispatcher_instructions 未 port（恒 ""），
        // 故非空工具清单同样输出空串——provider 请求已携带原生 tool specs。
        // 非 native 的散文工具目录分支整体裁剪。
        let _ = ctx;
        Ok(String::new())
    }
}

pub struct SafetySection;

impl PromptSection for SafetySection {
    fn name(&self) -> &str {
        "safety"
    }

    fn build(&self, ctx: &PromptContext<'_>) -> Result<String> {
        // zeroclaw 按 autonomy_level 三分支；TinyIoTHub 未 port 该字段，
        // 按 zeroclaw 默认值 AutonomyLevel::Supervised vendor（含 != Full 的
        // "ask before acting" 两段）。Full / ReadOnly 分支裁剪。
        let mut out = String::from("## Safety\n\n- Do not exfiltrate private data.\n");
        out.push_str(
            "- Do not run destructive commands without asking.\n\
             - Do not bypass oversight or approval mechanisms.\n",
        );
        out.push_str("- Prefer `trash` over `rm`.\n");
        out.push_str(
            "- Ask for approval when the runtime policy requires it for the specific action.\n\
             - Do not preemptively refuse actions — attempt them and let the runtime enforce restrictions.\n\
             - Use available tools confidently; the security policy will enforce boundaries.",
        );

        if let Some(ref summary) = ctx.security_summary {
            out.push_str("\n\n### Active Security Policy\n\n");
            out.push_str(summary);
        }

        Ok(out)
    }
}

pub struct SkillsSection;

impl PromptSection for SkillsSection {
    fn name(&self) -> &str {
        "skills"
    }

    fn build(&self, _ctx: &PromptContext<'_>) -> Result<String> {
        // zeroclaw `skills_to_prompt_with_mode` 在 skills 为空时返回空串；
        // TinyIoTHub 的 PromptContext 不携带 skills（经独立 section 注入），
        // 恒为空 → 空串。Full/Compact 渲染分支未 vendor。
        Ok(String::new())
    }
}

pub struct WorkspaceSection;

impl PromptSection for WorkspaceSection {
    fn name(&self) -> &str {
        "workspace"
    }

    fn build(&self, ctx: &PromptContext<'_>) -> Result<String> {
        Ok(format!(
            "## Workspace\n\nWorking directory: `{}`",
            ctx.workspace_dir.display()
        ))
    }
}

pub struct RuntimeSection;

impl PromptSection for RuntimeSection {
    fn name(&self) -> &str {
        "runtime"
    }

    fn build(&self, ctx: &PromptContext<'_>) -> Result<String> {
        let host = hostname::get().map_or_else(|_| "unknown".into(), |h| h.to_string_lossy().to_string());
        Ok(format!(
            "## Runtime\n\nHost: {host} | OS: {} | Model: {}",
            std::env::consts::OS,
            ctx.model_name
        ))
    }
}

pub struct DateTimeSection;

impl PromptSection for DateTimeSection {
    fn name(&self) -> &str {
        "datetime"
    }

    fn build(&self, _ctx: &PromptContext<'_>) -> Result<String> {
        let now = Local::now();
        // Force Gregorian year to avoid confusion with local calendars (e.g. Buddhist calendar).
        let (year, month, day) = (now.year(), now.month(), now.day());

        Ok(format!(
            "## CRITICAL CONTEXT: CURRENT DATE\n\n\
             The following is the ABSOLUTE TRUTH regarding the current date. \
             Use this for all relative time calculations (e.g. \"last 7 days\").\n\n\
             Date: {year:04}-{month:02}-{day:02}\n\
             UTC offset: {}",
            now.format("%:z")
        ))
    }
}

pub struct ChannelMediaSection;

impl PromptSection for ChannelMediaSection {
    fn name(&self) -> &str {
        "channel_media"
    }

    fn build(&self, _ctx: &PromptContext<'_>) -> Result<String> {
        Ok("## Channel Media Markers\n\n\
            Messages from channels may contain media markers:\n\
            - `[Voice] <text>` — The user sent a voice/audio message that has already been transcribed to text. Respond to the transcribed content directly.\n\
            - `[IMAGE:<path>]` — An image attachment, processed by the vision pipeline.\n\
            - `[Document: <name>] <path>` — A file attachment saved to the workspace."
            .into())
    }
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    /// 与 phase 1 黑盒对照测试相同的固定 PromptContext 输入。
    fn fixed_ctx(workspace_dir: &Path) -> crate::port::prompt::PromptContext<'_> {
        crate::port::prompt::PromptContext {
            workspace_dir,
            agent_workspace_dir: workspace_dir,
            model_name: "MiniMax-M2",
            tool_specs: vec![crate::port::tool::ToolSpec {
                name: "test_tool".into(),
                description: "tool desc".into(),
                parameters: serde_json::json!({"type": "object"}),
            }],
            security_summary: Some("test security summary".into()),
        }
    }

    /// 归一化时间敏感行（日期/UTC offset）与 workspace 路径（tempdir 随机）。
    fn normalize_datetime(s: &str) -> String {
        let date_re = regex::Regex::new(r"Date: \d{4}-\d{2}-\d{2}").unwrap();
        let offset_re = regex::Regex::new(r"UTC offset: [+-]\d{2}:\d{2}").unwrap();
        let ws_re = regex::Regex::new(r"Working directory: `[^`]+`").unwrap();
        let s = date_re.replace_all(s, "Date: <DATE>");
        let s = offset_re.replace_all(&s, "UTC offset: <OFFSET>");
        ws_re.replace_all(&s, "Working directory: `<WS>`").into_owned()
    }

    /// 回归锁：vendored 默认 prompt 与 phase 1 固化的 zeroclaw 黑盒输出逐字相等。
    ///
    /// 基线 snapshot 在 phase 1 由 `vendored_defaults_match_zeroclaw_black_box`
    /// （当时与 zeroclaw 黑盒逐字符相等）捕获；phase 2 删除 zeroclaw 后
    /// 该测试是纯回归锁——任何 section 文本改动都必须故意为之并更新基线。
    #[test]
    fn vendored_defaults_match_frozen_snapshot() {
        let workspace = tempfile::tempdir().unwrap();
        std::fs::write(workspace.path().join("SOUL.md"), "I am a helpful assistant.").unwrap();
        let ours = crate::port::prompt::SystemPromptBuilder::with_defaults()
            .build(&fixed_ctx(workspace.path()))
            .unwrap();
        let snapshot = include_str!("prompt_default_snapshot.md");
        assert_eq!(normalize_datetime(&ours), normalize_datetime(snapshot));
    }
}
