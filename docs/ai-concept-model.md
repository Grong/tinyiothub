# AI 概念模型（单一事实源）

> 本文档是 AI 子系统概念模型的单一事实源，是 Phase 2/3 内核归一任务的评审校准尺。
> 词汇表与 `docs/superpowers/specs/2026-10-12-ai-consolidation-design.md` §1 逐字一致；
> 词汇表右列（收编的异名）是 E3 概念模型 lint 脚本的异名清单来源。

## 概念链

```
触发（Trigger）──► 运行（Run）──► 判断（Verdict）──► 处置（Disposition）
  告警/定时/指令      一次 AI 工作单元      noise/self_healable/     自行闭环/待审批/
                     （trace+summary；         needs_human            转工单
                      narrative=summary 的
                      展示形态）
```

## 词汇表

| 概念 | 定义 | 收编的异名 |
|---|---|---|
| Run | 一次 AI 工作单元（trace+summary） | 调查 run / 心跳 tick / 指令触发的 run（UserDirective；chat 消息不落 agent_runs，chat 是独立形态） |
| Verdict | Run 的结构化结论 | verdict / outcome / status |
| Proposal | 需人批准的动作建议 | proposal / pending action / 待审批 |
| Disposition | 判断的处置结果 | disposition / judgment 状态机 |
| Trace | Run 的逐步工具轨迹 | trace / 证据 / evidence / 执行历史 |
| Trust Mode | 工作区自主级别（一等 UX，统一循环后门控全部 run——D9 裁定的行为变更） | trust_level（埋在配置页） |
| Narrative | Run 结论的大白话展示形态（D8 裁定：=summary 的展示形态，RunReport 不增字段） | 调查叙事 |

归一依据：T1/T9 已打通双 runner 的轨迹类型（ToolTraceEntry 共享），RunStarted/registry/trace 端点已按"run 是一等工作单元"落地——概念模型是对已有演化的命名收口，不是新发明。

## 概念→代码/表映射

| 概念 | 代码 | 表 |
|---|---|---|
| Run | crates/agent/src/runtime/thing_agent（runner.rs/manager.rs） | agent_runs |
| Verdict | apps/cloud/src/domains/agent/host/judgment_subscriber.rs（parse_verdict:88） | judgments |
| Proposal | crates/db/src/heartbeat.rs insert_action_row（582-604 行生产写入，由 host/persist.rs HeartbeatResultReady 支路驱动） | agent_actions（action_type='proposal'） |
| Trace | ToolTraceEntry（crates/core/src/agent_runs.rs） | agent_runs.report JSON |
| Trust Mode | apps/cloud/src/domains/agent/host/policy_engine.rs + host/tools/autonomous_invoke.rs（每动作新鲜读，O7） | workspaces + policy_rules |
| Narrative | =summary 的展示形态（D8，无独立字段） | — |
| 告警→Heartbeat 变体 | crates/agent/src/runtime/orchestrator/callbacks.rs HeartbeatBridge | — |

## 两个注册表的关系（eng S1-P3）

RunRegistry=记忆注入源（run_pipeline 读最近 5 条摘要进 prompt）；ActiveRunRegistry=实时 trace 源（trace 端点轮询）。职责不同，不合并。

- RunRegistry：`crates/agent/src/runtime/thing_agent/registry.rs`——每 workspace 保留最近已完成 run 窗口，T10 prompt 注入（manager.rs `run_pipeline`）只取最近 5 条 summary 与同 dedup_key 最近 3 条历史。
- ActiveRunRegistry：`crates/agent/src/runtime/thing_agent/active_runs.rs`——in-flight run 的实时轨迹，`apps/cloud/src/domains/agent/trace_handler.rs` 轮询。
