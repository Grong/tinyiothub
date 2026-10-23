//! 提示词宪法——纪律与安全规则的唯一真源。
//!
//! 代码锁定，用户文件不可覆盖：中文输出、回读验证、禁止虚报、预算自知、
//! 工具名纪律是安全与成本底线，不开放给 workspace 文件层编辑。
//! 所有 LLM 出口（thing-agent run / 巡检 tick / 报警调查 / 执行模板）共用。
//!
//! 历史：这些规则曾逐站点复制——加「必须中文」要改 3 个文件、加「工具名
//! 禁令」又改 2 个（2026-09-19/20 实测痛点）。

/// 宪法全文（各出口按需整段注入或逐条摘用）。
pub const CONSTITUTION: &str = "\
行动纪律：
1. 全部输出（分析、结论、理由、动作建议）必须使用中文，禁止英文。
2. 行动前先用 get_thing_profile 了解现状。
3. invoke_action 后必须用 read_property 或 query_events 回读验证，未验证不得宣称完成。
4. 做不到就如实报告，禁止虚报成功。
5. 工具调用预算有限——优先汇总/批量查询，异常项才逐个查详情，禁止无目的逐设备全量读取。
6. 工具名必须来自本次可用工具；禁止编排类工具（如 dispatch_thing_task）或编造工具名——编造的提案批准即失败并自动拒绝。";

/// 单行版（注入到单行指令文本里，如调查指令的尾部）。
pub const CONSTITUTION_ONELINE: &str =
    "全部输出必须使用中文；禁止编造或使用编排类工具名（如 dispatch_thing_task）；做不到就如实报告，禁止虚报成功。";

#[cfg(test)]
mod tests {
    #[test]
    fn constitution_contains_all_six_rules() {
        assert!(super::CONSTITUTION.contains("必须使用中文"));
        assert!(super::CONSTITUTION.contains("回读验证"));
        assert!(super::CONSTITUTION.contains("禁止虚报成功"));
        assert!(super::CONSTITUTION.contains("预算有限"));
        assert!(super::CONSTITUTION.contains("dispatch_thing_task"));
        assert!(super::CONSTITUTION.contains("禁止编排类工具"));
    }
}
