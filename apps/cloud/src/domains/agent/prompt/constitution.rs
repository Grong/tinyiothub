//! 提示词宪法——纪律与安全规则的唯一真源。
//!
//! 代码锁定，用户文件不可覆盖：中文输出、回读验证、禁止虚报、预算自知、
//! 工具名纪律是安全与成本底线，不开放给 workspace 文件层编辑。
//! 所有 LLM 出口（thing-agent run / 巡检 tick / 报警调查 / 执行模板）共用。
//!
//! 宪法只写**通用**工具使用规则，绝不点名具体工具：工具清单、说明、审批与
//! 脱敏全部由 `ToolDescriptor` 生成，新增工具不需要修改这里（2026-11-03 T9）。
//!
//! 历史：这些规则曾逐站点复制——加「必须中文」要改 3 个文件、加「工具名
//! 禁令」又改 2 个（2026-09-19/20 实测痛点）。

/// 宪法全文（各出口按需整段注入或逐条摘用）。
pub const CONSTITUTION: &str = "\
行动纪律：
1. 全部输出（分析、结论、理由、动作建议）必须使用中文，禁止英文。
2. 行动前先读取现状：设备状态用只读工具查询，结构关系用层级/列表工具确认，禁止凭空推断。
3. 执行动作后必须用只读工具回读验证，未验证不得宣称完成。
4. 做不到就如实报告，禁止虚报成功。
5. 工具调用预算有限——优先汇总/批量查询，异常项才逐个查详情，禁止无目的逐设备全量读取。
6. 工具名必须来自本次可用工具；禁止编造工具名或调用未暴露的内部/编排工具——编造的提案批准即失败并自动拒绝。";

/// 单行版（注入到单行指令文本里，如调查指令的尾部）。
pub const CONSTITUTION_ONELINE: &str =
    "全部输出必须使用中文；禁止编造或使用未暴露的内部/编排工具名；做不到就如实报告，禁止虚报成功。";

#[cfg(test)]
mod tests {
    #[test]
    fn constitution_contains_all_six_rules() {
        assert!(super::CONSTITUTION.contains("必须使用中文"));
        assert!(super::CONSTITUTION.contains("回读验证"));
        assert!(super::CONSTITUTION.contains("禁止虚报成功"));
        assert!(super::CONSTITUTION.contains("预算有限"));
        assert!(super::CONSTITUTION.contains("禁止编造工具名"));
        assert!(super::CONSTITUTION.contains("禁止无目的逐设备全量读取"));
    }

    /// 新增工具只改 descriptor；宪法不得点名任何具体工具，否则又会退化成
    /// 手同步清单。工具可见性由 ToolDescriptor 驱动的注册表负责。
    #[test]
    fn constitution_does_not_hardcode_tool_names() {
        for tool in [
            "read_device",
            "call_device",
            "set_device",
            "get_thing_tree",
            "list_devices",
            "query_events",
            "dispatch_thing_task",
        ] {
            assert!(
                !super::CONSTITUTION.contains(tool),
                "constitution must not hardcode tool name `{tool}`"
            );
        }
        assert!(super::CONSTITUTION.contains("本次可用工具"));
    }
}
