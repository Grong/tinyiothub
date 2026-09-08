# Agent 引擎迁移决策记录：zeroclaw → rig

- 日期：2026-09-06/07
- 状态：已落地（见下文 commit 序列）
- 范围：`crates/agent` 引擎层 + `apps/cloud` 调用点

## 动机

zeroclaw 是**应用**而非库。我们以 git 依赖 + vendor 方式消费它，持续被其应用层决策反噬：

- **v0.8.1 私有化 security 模块**：上游把 `security` 模块改为 crate-private，我们依赖的 `SecurityPolicy` 摘要直接断链，迫使维护一个 2 行 diff 的 fork 仅为了重新导出。
- **NamespacedMemory 移除先例**：上游此前无预警移除 `NamespacedMemory`，我们已被迫做过一次记忆层自救。
- 应用型上游的迭代节奏（CLI 功能、gateway 形态）与我们需要的"agent loop 库"错位，每次升级都要甄别大量无关变更。

结论：不再消费应用型上游，迁到库优先的引擎。

## 选型对比

| 方案 | 评估 | 结论 |
|------|------|------|
| 维持 zeroclaw | 持续 fork 维护成本；上游应用化方向不可逆 | 否决 |
| **rig (rig-core/rig-agent 0.42)** | 库优先设计；MIT；内置 minimax provider（我们在用）；rig-rmcp 覆盖后续 MCP 工具接入；streaming/multi-turn/hook 齐备 | **选定** |
| 自研 loop | 完全可控但需自实现 streaming、multi-turn、provider 适配矩阵，工程量与验证成本最高 | 否决 |
| genai + 自研 loop | genai 只做 completion 抽象，tool loop 仍要自研，等于上一方案的变体 | 否决 |

## port 反腐败层设计

不直接把 rig 类型泄漏进业务代码。`crates/agent/src/port/` 定义自有接口面（形状 vendored 自已验证合理的 zeroclaw-api 设计，既有工具实现与 ScriptedProvider 已长在该形状上）：

| 模块 | 内容 |
|------|------|
| `port::tool` | `Tool` trait + `ToolResult`（`spec()` 默认方法） |
| `port::provider` | `ModelProvider` trait + `ChatMessage`/`ChatRequest`/`ToolCall` |
| `port::runtime` | `AgentLoop` trait（`turn_streamed`/`run_single`/`clear_history`/`seed_history`）、`AgentLoopConfig`、`AgentLoopHandle`、`MAX_LOOP_TURNS=25` |
| `port::memory` | `Memory` trait（store/recall/forget）+ `NoopMemory` |
| `port::observer` | `Observer` trait + `NoopObserver` |
| `port::prompt` / `port::prompt_sections` | `SystemPromptBuilder` + 9 个默认 section（vendored，快照回归锁 `prompt_default_snapshot.md`） |
| `port::events` | `TurnEvent`（Chunk/Thinking/ToolCall/ToolResult/ApprovalRequest/Usage） |
| `port::attribution` | `Attributable`/`Role`/`ToolKind`（provider/tool 归因） |
| `port::outcome` | `ToolLoopCancelled` 取消哨兵（见下） |

`adapters/rig/` 是 port → rig 的唯一桥：`loop_.rs`（AgentLoop）、`provider.rs`（CompletionModel + canonical 编码）、`tools.rs`（DynamicTool）、`memory.rs`（ConversationMemory）。业务代码只依赖 port。

## canonical 消息编码契约

引擎与 provider 之间交换的历史是纯文本 `ChatMessage`，结构化内容（assistant 的 tool_calls、tool result）按固定 JSON 编码进文本。契约与 zeroclaw dispatcher 序列化历史时使用的格式逐字节一致（来源：vendor `zeroclaw/agent/dispatcher.rs:229-260`，已随 fork 删除；现由 `adapters/rig/provider.rs` 双向实现并锁测试）：

- **assistant 带工具调用** → 一条 `role=assistant` 消息，content 为：
  ```json
  {"content": "文本或 null", "tool_calls": [{"id","name","arguments"}]}
  ```
  `arguments` 是 `Value::to_string()` 产出的 JSON 字符串；有 reasoning 时附加 `"reasoning_content"` 键。
- **tool result** → 一条 `role=tool` 消息，content 为：
  ```json
  {"tool_call_id": "...", "content": "文本化结果"}
  ```
  content 为 Text 块拼接；Json 块经 serde 序列化。
- system/user 纯文本不编码，直传。

反向（port → rig）按同一编码解析；非 canonical 文本（旧库中 zeroclaw 时代持久化的纯文本 assistant）按原样回退为对应角色纯文本消息。多媒体块（Image/Audio/Video/Document）无 canonical 文本位，拍平时跳过、反向不可恢复（文档化差异）。

该契约保证：同一 provider（含测试用 ScriptedProvider）在迁移前后看到完全相同的历史文本；持久化在 DB 中的旧历史在新引擎下仍可正确复原。

## 取消语义归一化

rig 与 zeroclaw 对"工具循环被打断"的 surfacing 不同。适配器统一归一为哨兵错误 `port::outcome::ToolLoopCancelled`：runner 的 `CancellationToken` 触发、`MAX_LOOP_TURNS` 预算超支、BudgetHook 停止，全部映射为 `Err(ToolLoopCancelled)`，runner 用 `is_tool_loop_cancelled`（沿 error chain 查找）判定，不会把预算超支误判为正常 `TurnEnd::Text`。

## conversation_memory 开关

`AgentLoopConfig::conversation_memory` 控制是否给引擎接线 rig `ConversationMemory`（自动 load/append）：

- `false`（chat/heartbeat 路径）：历史由 cloud 侧 DB 每轮重建（`clear_history` + `seed_history` 注入内部缓冲，逐轮 `.history(...)` 传入），不配 ConversationMemory，避免 seed 与引擎自动 load/append 的重复累积。
- `true`（thing_agent 自治路径）：走 `PortMemoryAsConversation`（port Memory 承载的会话内存），保留跨轮 recall 行为。

## 可接受行为差异清单

以下差异经评审裁定可接受（全局约束 #6），即 PR 描述中的同一清单：

1. **response_cache 死代码删除**：zeroclaw 的响应缓存无人消费，不移植。
2. **失败工具回传 JSON 信封**：port `ToolResult{success:false}` 以 `{"success":false,"error":...}` JSON 文本作为普通 tool result 回传（zeroclaw 时代为 `"Error: {reason}"` 纯文本），不让 rig 走错误重试路径；仅 `execute()` 自身的 `Err` 映射为 `ToolExecutionError`。
3. **rig Usage cached 未上报 Some(0)**：rig 的 cached token 语义与 zeroclaw 不完全对齐，宁可缺省（None = 不可用）不虚报 0。
4. **recall 注入形式**：迁移后为结构化 conversation（逐条消息复原），zeroclaw 时代为模糊 context 文本拼接。
5. **store 无 (agent_id,key) upsert**：port Memory 的 store 是 append-only；同 key 重复写会产生重复条目——已登记后续去重任务。
6. **observer Noop**：rig 侧观测接线（hooks）未完成，现阶段 Observer 为 Noop，指标/追踪事件暂不上报。
7. **RECALL_LIMIT=20 与默认会话 id**：`PortMemoryAsConversation::load` 空查询召回最近 20 条（反转为时间正序）；现状各 turn 独立无显式会话，固定 `CONVERSATION_ID="default"`。
8. **ToolCall 事件事后语义**：工具调用事件取自 rig `ToolExecutionCommitted`（hook 之后、每次调用恰一次），是"已提交"语义而非"即将执行"语义。
9. **多媒体块拍平跳过**：canonical 编码无多媒体位（见上节）。

## 回滚方式

迁移为纯追加式 commit 序列，无破坏性 schema 变更。回滚 = revert 本分支迁移 commit 序列：

```
git revert 71122eb0^..4774d07d
```

覆盖范围（旧 → 新）：

| commit | 内容 |
|--------|------|
| `71122eb0` | 引入 rig-core/rig-agent 0.42.0 依赖 |
| `0d37f80c` | port 骨架（attribution/Tool/provider/TurnEvent/取消语义） |
| `eb82f6c1` | port prompt（9 默认 section vendored + 黑盒保真对照） |
| `b798123d` | port runtime（AgentLoop/Memory/Observer 接口面） |
| `10810c78` | zeroclaw 适配器机械桥 |
| `6f8fb2e1` | provider 双向桥 + loop 工厂 |
| `9f3ba95c` | ScriptedProvider 迁移 port ModelProvider |
| `62378583` | 全部调用点迁移 port 接口 + canonical 锁 |
| `2dca437d` | rig 适配器（DynamicTool/CompletionModel/ConversationMemory + TurnEvent 翻译） |
| `2e39eb05` | 记忆正序 + 取消竞态 + conversation_memory 开关 |
| `8869b472` | abort_spike 示例补字段 |
| `f43b50b6` | 默认引擎切换到 rig |
| `634d9ad6` | 移除 zeroclaw 依赖与 fork |
| `026536f7` | 快照指纹归一化 + ci.yml 残留清理 |
| `4774d07d` | JsonlMemory 恢复自治路径持久化记忆 |

注意：revert `634d9ad6` 会恢复 zeroclaw git 依赖，需同时恢复 vendor/fork 仓库可达性。持久化数据（DB 中 canonical 编码的历史消息）两个引擎均可读取，无需数据迁移。
