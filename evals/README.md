# 判断质量 Eval 套件（T11）

AI 报警分诊的判断质量基线。手动运行，不进 CI（真 LLM 调用必 flaky）。

## 运行

```bash
MINIMAX_API_KEY=... cargo test -p tinyiothub-cloud judgment_eval -- --ignored --nocapture
# 可选：MINIMAX_BASE_URL / MINIMAX_MODEL 覆盖
```

## 验收门槛（D7 裁决）

- verdict 分类准确率 ≥ 85%
- needs_human → noise 误判 = 0 容忍（漏掉真人该管的事是不可接受方向；
  反方向（noise→needs_human）只是多一张工单，可接受）

## 场景库（scenarios.json）

24 条标注场景：8 噪声（noise）/ 6 可自愈（self_healable）/ 8 需人工
（needs_human）/ 2 模糊边界（edge）。每条含事实描述 + 期望 verdict +
标注理由（note）。

## 变更规程

- 改调查 prompt（callbacks.rs `alarm_investigation_text`）后重跑对比基线
- 影子期攒下的真实「对/错」反馈应回流为新的标注场景（反馈即训练数据）
