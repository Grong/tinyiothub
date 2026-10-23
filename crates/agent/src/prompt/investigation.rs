//! 调查指令文本——报警调查（从 orchestrator/callbacks.rs 迁入，2026-09-20
//! 提示词收敛）。心跳提案 directive 文本已随 X6 心跳桥自动派发退役
//! （2026-10-16 用户裁定）删除——提案一律走审批队列，不再自动派修复 run。

use tinyiothub_core::models::event::AlarmEvent;

use crate::prompt::constitution::CONSTITUTION_ONELINE;

/// T3：报警调查指令。要求 agent 调查后给出结构化判断（judgment subscriber
/// 解析 summary 尾部的 ```json verdict 块；解析失败按 outcome 兜底）。
/// pub：eval 套件（judgment_eval_tests）用同一模板保证 prompt parity。
///
/// 2026-09-16：携带规则条件描述（condition_desc）——实测 AI 在设备属性里
/// 找不到阈值只能猜（"设备没有 alarm_threshold 属性"），三个 run 全部
/// no_action_needed。条件直接给，不让 AI 猜。
pub fn alarm_investigation_text(alarm: &AlarmEvent) -> String {
    let condition = alarm
        .condition_desc
        .as_deref()
        .map(|c| format!("触发条件：{c}。"))
        .unwrap_or_default();
    format!(
        "调查报警并给出处置判断。报警：{}（设备 {}，类型 {}，级别 {}）。{}\
         请查询设备状态与近期事件后判断（查不到设备数据时基于已有信息直接判断）：\
         noise（异常已自行消失或当前状态正常——越限后回落且平稳、闪断已重连、节律性读数、\
         与其他传感器交叉验证不符的已知漂移；无需任何动作）/ \
         self_healable（异常仍存在，且有已知低风险可逆的恢复动作——重连/重试/重启模块——\
         历史同类动作曾生效；给出建议动作）/ \
         needs_human（恢复动作已失败、历史模式指向现场处置、或根因不明且影响持续）。\
         判据要点：已自行恢复的是 noise 不是 self_healable（self_healable 要求问题仍然存在）；\
         不确定时选 needs_human。\
         {CONSTITUTION_ONELINE}\
         结束前输出一行结构化结论：```json {{\"verdict\": \"...\", \"reason\": \"一句人话理由\", \
         \"suggested_action\": \"建议动作或 null\", \"action_category\": \"device_reboot|connection_recovery|property_adjust|threshold_tuning|other\", \
         \"narrative\": \"调查叙事\"}}```。\
         narrative 必填：≤200 字、第一人称大白话讲清你查了什么、看到什么、为何这么判（如\"我检查了温度曲线，三次尖峰都自行回落\"）；\
         调查叙述只能写在 json 块的 narrative 字段内，块外禁止散文叙述调查过程。",
        alarm.message, alarm.thing_id, alarm.alarm_type, alarm.severity, condition
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use tinyiothub_core::models::event::AlarmEvent;

    #[test]
    fn investigation_text_carries_condition_and_constitution() {
        let alarm = AlarmEvent {
            id: "a1".into(),
            workspace_id: "ws".into(),
            thing_id: "d1".into(),
            alarm_type: "threshold".into(),
            severity: "warning".into(),
            message: "温度过高".into(),
            rule_id: Some("r1".into()),
            condition_desc: Some("阈值 > 10".into()),
            resolved: false,
            created_at: chrono::Utc::now(),
        };
        let text = alarm_investigation_text(&alarm);
        assert!(text.contains("触发条件：阈值 > 10。"), "条件段缺失: {text}");
        assert!(text.contains("全部输出必须使用中文"), "宪法单行缺失: {text}");
        assert!(text.contains("dispatch_thing_task"), "工具名禁令缺失: {text}");
        // 2026-09-28 eval 质量修复：判据区分点 + fail-safe。
        assert!(
            text.contains("已自行恢复的是 noise 不是 self_healable"),
            "判据要点缺失: {text}"
        );
        assert!(text.contains("不确定时选 needs_human"), "fail-safe 缺失: {text}");
        // Task 7：verdict 协议 narrative 字段约束——必填、≤200 字、第一人称
        // 大白话、只能在 ```json verdict 块内（块外禁止散文叙述）。
        assert!(text.contains("\"narrative\""), "narrative 字段缺失: {text}");
        assert!(text.contains("narrative 必填"), "narrative 必填约束缺失: {text}");
        assert!(text.contains("≤200 字"), "narrative 长度约束缺失: {text}");
        assert!(text.contains("第一人称"), "narrative 人称约束缺失: {text}");
        assert!(text.contains("块外禁止散文"), "narrative 块外禁散文约束缺失: {text}");
    }
}
