# Eval 套件（T11 判断质量 + T12 Harness 编排）

`evals/scenarios.json` 是两类 eval 的单一场景源：

- `judgment`：AI 报警分诊判断质量（T11）。weekly CI 定时跑
  （`.github/workflows/judgment-eval-weekly.yml`，非阻断哨兵，E1），也可手动运行。
- `harness`：Harness 工具/Workflow/Effect/TaskOutcome 编排（T12）。scripted
  场景在 `cargo test -p tinyiothub-cloud --test runtime_eval` 中默认运行；
  live-provider 场景是 ignored/opt-in，永不进 CI。

## 判断质量 eval（judgment）

```bash
MINIMAX_API_KEY=... cargo test -p tinyiothub-cloud judgment_eval -- --ignored --nocapture
# 可选：MINIMAX_BASE_URL / MINIMAX_MODEL 覆盖
```

### 保真约定（F-E/T-11，2026-09-14 工程评审）

eval 与生产**共用同一 prompt 模板**（`callbacks.rs alarm_investigation_text`）与
**同一解析函数**（`judgment_subscriber::parse_verdict`）——不维护手抄副本。
改调查 prompt 或解析规则会直接进入本 eval；基线永远是生产管线的度量。

### 验收门槛（D7 裁决）

- verdict 分类准确率 ≥ 85%
- needs_human → noise 误判 = 0 容忍（漏掉真人该管的事是不可接受方向；
  反方向（noise→needs_human）只是多一张工单，可接受）

### 判断场景库（`scenarios.json` 的 `judgment`）

24 条标注场景：8 噪声（noise）/ 6 可自愈（self_healable）/ 8 需人工
（needs_human）/ 2 模糊边界（edge）。每条含事实描述 + 期望 verdict +
标注理由（note）。

## Harness 编排 eval（harness）

```bash
# scripted：默认运行，mock 只验证编排，不宣称 live 模型质量或真实设备安全
cargo test -p tinyiothub-cloud --test runtime_eval -- --nocapture

# live：只有已授权 staging provider 配置时才运行；结果单独记录
TINYIOTHUB_EVAL_BASE_URL=... \
TINYIOTHUB_EVAL_API_KEY=... \
cargo test -p tinyiothub-cloud --test runtime_eval live_provider_eval_opt_in -- --ignored --nocapture
# 可选：TINYIOTHUB_EVAL_MODEL / TINYIOTHUB_EVAL_PROVIDER
```

live 测试与 scripted 测试读取**同一批 `harness` 场景**，每次运行打印
`live provider=... model=... version=... scenario=...`。没有授权配置时输出
`live=not run` 并退出；live 结果不写回 scripted 断言或生产数据。

### 覆盖边界

- 新 `ToolDescriptor` 工具自动进入 LLM 工具子集与 catalog，无需改硬编码 allowlist。
- 非视觉 provider 在模型调用前失败关闭：`capability.unavailable`、零 Effect。
- `denied` 审批不 dispatch；重复恢复不产生第二个副作用或 tool result。
- `unknown` Effect 不重发、不伪造完成；对账决定只创建一次。
- ACK/命令接收只表示 accepted/dispatching，不等于物理完成；完成需要设备结果/证据。
- `TaskOutcome` 保持 camelCase schema，`evidenceRefs` 原样保留。

### 变更规程

- 改调查 prompt（callbacks.rs `alarm_investigation_text`）后重跑判断基线。
- 改 ToolDescriptor、Effect/Decision 门或 Workflow runner 时，先更新
  `harness` 场景的 input/expected/forbiddenSideEffects，再补实现。
- 影子期攒下的真实「对/错」反馈应回流为新的标注场景（反馈即训练数据）。
- 不得为通过 mock 而绕过 Effect/审批门，也不得用 mock 通过推断 live 模型质量。
