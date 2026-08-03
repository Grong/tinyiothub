# SPIKE_RESULT — Thing Agent Loop T0: Abort go/no-go

**结论：GO** — CancellationToken 能真正停掉 zeroclaw 的 LLM tool loop（包括进行中的工具调用）。Task 9 的 runner 采用 **A 方案（流式 abort）**，无需 B 方案的工具内计数拒绝。

日期：2026-07-29 ｜ zeroclaw：git tag `v0.8.1-patched`（checkout `12f5360`）

## 验证问题

1. `turn_streamed` 第三参是否接受 `Option<CancellationToken>`？→ **是**
2. abort 后返回是否为 `Err(ToolLoopCancelled)`？→ **是**
3. 进行中的慢工具（sleep 30s）是否被立即丢弃而非等待完成？→ **是**

## 关键签名（源码摘录）

`crates/zeroclaw-runtime/src/agent/agent.rs:2049-2054`：

```rust
pub async fn turn_streamed(
    &mut self,
    user_message: &str,
    event_tx: tokio::sync::mpsc::Sender<TurnEvent>,
    cancel_token: Option<tokio_util::sync::CancellationToken>,
) -> Result<(String, Vec<ConversationMessage>)>
```

注意：返回类型是 `Result<(String, Vec<ConversationMessage>)>`（anyhow::Result），不是 brief 里说的 `(String, Conversation)`。

## 取消错误类型

`crates/zeroclaw-runtime/src/agent/turn/outcome.rs:10-22`：

```rust
#[derive(Debug)]
pub struct ToolLoopCancelled;

impl std::fmt::Display for ToolLoopCancelled {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("tool loop cancelled")
    }
}

impl std::error::Error for ToolLoopCancelled {}

pub fn is_tool_loop_cancelled(err: &anyhow::Error) -> bool {
    err.chain().any(|source| source.is::<ToolLoopCancelled>())
}
```

外部可达路径（facade `zeroclaw` crate）：`zeroclaw::agent::loop_::{ToolLoopCancelled, is_tool_loop_cancelled}`
（`zeroclaw-runtime/src/agent/loop_.rs:693-697` pub re-export；`turn` 模块本身是 `pub(crate)`，不能从 `agent::turn` 访问）。

检测方式与 zeroclaw 自身一致：`is_tool_loop_cancelled(&err)`（gateway `ws.rs:1192`、ACP `acp_server.rs:1383` 均如此）。

## 取消机制（为什么有效）

`crates/zeroclaw-runtime/src/agent/tool_execution.rs:178-183` — 工具 future 与 token 被 `tokio::select!` 竞争：

```rust
let tool_result = if let Some(token) = cancellation_token {
    tokio::select! {
        () = token.cancelled() => return Err(ToolLoopCancelled.into()),
        result = tool_future => result,
    }
} else {
    tool_future.await
};
```

即 cancel 时进行中的工具 future 被直接 drop，`Err(ToolLoopCancelled)` 沿调用栈传播出 `turn_streamed`。工具循环入口也有 cancel 前置检查（`turn/mod.rs:253-257`），批次中途取消见 `tool_execution.rs:411-445`。

## Spike 实现与结果

位置：`cloud/examples/abort_spike.rs`（不在 `crates/tinyiothub-ai/examples/`——tinyiothub-ai 尚无 zeroclaw 依赖，那是 Task 2 的事；cloud 已依赖 zeroclaw，spike 零依赖改动即可编译运行，仅给 cloud 加了 dev-dependency `tokio-util = "0.7"`）。

- 假 provider（`ScriptedProvider`，仿 zeroclaw 自带测试的 `ScriptedModelProvider`，见 `zeroclaw-runtime/src/agent/tests.rs:51`）：固定返回一次 `slow_tool` 的 native ToolCall
- 慢工具 `SlowTool`：`execute` 里 `sleep(30s)`
- 收到第一个 `TurnEvent::ToolCall` 后 `token.cancel()`

运行：

```bash
cargo run -p tinyiothub-cloud --example abort_spike
```

输出：

```
[spike] ToolCall event: slow_tool -> cancelling token
[spike] elapsed: 21.13125ms
[spike] error: tool loop cancelled
[spike] PASS: cancel aborted the tool loop in 21.13125ms (tool alone sleeps 30s) => GO
```

断言全部通过：① 返回 `Err` ② `is_tool_loop_cancelled` 为 true ③ 总耗时 ~21ms << 30s（远小于 5s 预算）。

## 对 Task 9 的含义

- runner 用 `turn_streamed(msg, event_tx, Some(token))`，把 token 与 RunContext 绑定
- 硬预算（25 次工具调用 / 5min）触发时调 `token.cancel()`，循环立即停
- 取消判定用 `zeroclaw::agent::loop_::is_tool_loop_cancelled(&err)`，不要字符串匹配
- 注意 `TurnEvent::ToolCall` 在工具 dispatch 时发出（fire-and-forget，事件送达不保证工具已结束）；取消后在途工具被 drop，其副作用若已开始则不回滚——破坏性工具仍应走审批（ApprovalRequest 事件已存在）
