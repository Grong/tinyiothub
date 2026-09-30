//! 执行动作模板——按 action_category 生成审批执行指令文本。
//!
//! 从 `crates/db/src/judgment.rs` 赎回（2026-09-20）：提示词话术属于
//! prompt 层，不属于存储层——此前住在 db 是分层违规。

/// 按 action_category 生成执行指令文本（服务端模板，不信任 LLM 的
/// suggested_action——注入面收敛，4A/T-3）。
pub fn exec_action_template(category: Option<&str>, thing_id: Option<&str>) -> String {
    let thing = thing_id.unwrap_or("目标设备");
    match category {
        Some("device_reboot") => format!("重启设备 {thing}"),
        Some("connection_recovery") => format!("恢复设备 {thing} 的连接（重连/重订阅）"),
        Some("property_adjust") => format!("调整设备 {thing} 的属性设置"),
        Some("threshold_tuning") => "调整报警规则阈值".to_string(),
        _ => "按判断建议处置".to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn template_by_category() {
        assert_eq!(
            exec_action_template(Some("device_reboot"), Some("dev1")),
            "重启设备 dev1"
        );
        assert_eq!(exec_action_template(Some("threshold_tuning"), None), "调整报警规则阈值");
        assert_eq!(exec_action_template(Some("unknown"), Some("dev2")), "按判断建议处置");
        assert_eq!(exec_action_template(None, None), "按判断建议处置");
    }
}
