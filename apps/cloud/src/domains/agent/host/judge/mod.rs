//! judge/ — 判定链：LLM 裁决订阅投影（judgment_subscriber）+ 统一策略门（policy_engine）。
//! 谁调用：service_manager 装配 `supervise_judgment_subscriber`；judgment handler /
//! runtime_ports / 测试经 `project`/`broadcast_judgment_pub` 进入；trigger/thing_action_hooks
//! 与 tools 确认门消费 SqlitePolicyEngine。
//! 依赖谁：runtime 事件总线（AgentEvent 订阅）、tinyiothub_llm（裁决）、tinyiothub_policy
//! （规则评估）、storage（投影落库）；开票经 ticket 域 escalation seam，不反向依赖 notify/。

pub mod judgment_subscriber;
pub mod policy_engine;
