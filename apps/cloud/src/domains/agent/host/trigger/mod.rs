//! trigger/ — 触发链：用户指令投递桩（directive_sink）+ thing 域动作钩子
//! （thing_action_hooks）+ tenant 域 agent 钩子（agent_hooks）。
//! 谁调用：thing 域 HTTP handler 经 ThingActionHooks trait 消费动作确认门；
//! tenant 域经 AgentHooks trait 取默认心跳任务；指令端点/chat 工具测试用 StubDirectiveSink；
//! 三者的装配都在 state.rs / service_manager.rs。
//! 依赖谁：judge/policy_engine（确认门）、tools/thing（pending-action 存储与参数校验）、
//! heartbeat（默认任务集）、tinyiothub_agent runtime（DirectiveSink trait / AgentRuntime）。

pub mod agent_hooks;
pub mod directive_sink;
pub mod thing_action_hooks;
