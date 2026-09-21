//! 巡检 tick 提示词（从 runtime/heartbeat/loop_.rs 迁入，2026-09-20 提示词收敛）。

use tinyiothub_core::heartbeat::{HeartbeatTask, TrustConfig};

use crate::port::runtime::MAX_LOOP_TURNS;
use crate::prompt::constitution::CONSTITUTION;

/// 组装巡检 tick 提示词：任务列表 + 工作区原则（用户文件层，空串省略）+
/// 宪法段（纪律唯一真源）+ JSON 报告模板。
/// JSON 键保持英文（线格式，代码解析，非用户可见文本）。
pub fn build_heartbeat_prompt(
    workspace_id: &str,
    principles: &str,
    tasks: &[&HeartbeatTask],
    trust_config: &TrustConfig,
) -> String {
    let tasks_text: String = tasks
        .iter()
        .map(|t| format!("- [{}] {}", t.priority, crate::memory::reflect::sanitize_input(&t.text)))
        .collect::<Vec<_>>()
        .join("\n");
    let principles_text = if principles.is_empty() {
        String::new()
    } else {
        format!("\n\n{principles}")
    };

    format!(
        "你是工作区 {ws_id} 的 IoT 巡检 Agent。\n\
         信任级别：{trust:?}\n\
         每次 tick 最多自动执行动作数：{max}\n{principles_text}\n\n\
         ## 任务：\n{tasks}\n\n\
         ## {constitution}\n\n\
         逐项执行任务，输出 JSON 报告：\n\
         ```json\n\
         {{\n  \"status\": \"complete|partial|error\",\n  \
         \"summary\": \"...\",\n  \
         \"executed_actions\": [{{\"tool_name\": \"...\", \"thing_id\": \"...\", \"success\": true, \"details\": \"...\"}}],\n  \
         \"proposals\": [{{\"tool_name\": \"...\", \"thing_id\": \"...\", \"summary\": \"...\", \"reason\": \"...\", \"risk\": \"low|medium|high\", \"parameters\": {{...}}}}],\n  \
         \"error\": null\n}}\n```",
        ws_id = workspace_id,
        trust = trust_config.trust_level,
        max = trust_config.max_auto_actions_per_tick,
        tasks = tasks_text,
        constitution = CONSTITUTION,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_task() -> HeartbeatTask {
        HeartbeatTask {
            id: 1,
            workspace_id: "ws".into(),
            priority: "high".into(),
            text: "check devices".into(),
            paused: false,
            version: 1,
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
        }
    }

    #[test]
    fn prompt_asks_proposals_for_parameters() {
        // Without parameters the approve-and-execute flow has nothing to run.
        let task = sample_task();
        let prompt = build_heartbeat_prompt("ws", "", &[&task], &TrustConfig::default());
        assert!(
            prompt.contains("\"parameters\""),
            "proposal schema in the prompt must request tool parameters"
        );
    }

    #[test]
    fn prompt_injects_constitution() {
        // 纪律规则唯一真源是宪法段——巡检提示词必须注入（预算/中文/工具名纪律）。
        let task = sample_task();
        let prompt = build_heartbeat_prompt("ws", "", &[&task], &TrustConfig::default());
        assert!(prompt.contains("行动纪律"), "constitution must be injected");
        assert!(prompt.contains("必须使用中文"), "Chinese rule from constitution");
        assert!(
            prompt.contains("dispatch_thing_task"),
            "tool-name ban from constitution"
        );
    }
}
