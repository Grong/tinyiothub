# PR #88 CEO 合并前审查报告（SELECTIVE EXPANSION → 裁定：全量批量修复）

- 日期： 2026-09-03 | 分支： `refactor/thing-followups` → main | PR #88（30 commits, +8922/−128）
- 方法： 系统审计 + Step 0 范围挑战 + 4 维并行深审（backend-core / api-security / frontend / tests-edge）+ outside voice（Claude subagent；Codex CLI vendor 二进制缺失， ENOENT）
- 前置上下文： 规划期 CEO 审查（EXPANSION, E2/E4/E5/E6 采纳）+ 工程评审 + 3 轮对抗性复审 + 终审 CLEAN TO MERGE（rename 半区）
- 发现总量： **3 critical / 14 important / 32 minor**（去重后），门外声音新增 4 条实质发现
- 裁定： 用户选 **A 全量批量修复**（2026-09-03）；CEO plan: `~/.gstack/projects/Grong-tinyiothub/ceo-plans/2026-09-03-pr88-merge-review.md`

## 关键结论

**架构与契约方向正确，但四轮计划期审查漏掉了一个运行时 serde 失配：旗舰内置包 smart_campus 的告警 condition 不符合 `AlarmCondition` 形状，每次实例化提交必 400。** 根因模式：所有测试用内联模板 fixture，没有任何测试实例化真实 seed 内容；dry-run 路径不解析 condition，预览与提交分叉。同类问题集中在 scene_ref（E6）：传递引用不加载、目标默认值不合并、映射值不做 min/max——组合能力只有单测手工构造的 map 里能跑通。

## 系统架构图（本 PR 新增部分）

```
  前端 Lit                    cloud API                     db 层
 ┌──────────────┐   POST instantiate (dry_run?)  ┌────────────────────┐
 │ 场景包 Tab    │ ─────────────────────────────▶ │ SceneInstantiator  │
 │ 参数对话框    │   GET detail (parameters)      │  1. 加载+校验        │
 │ 300ms 防抖   │ ◀── {node_count,tree_preview, ─┤  2. expand() 纯函数 │──▶ scene_template.rs
 │ dry-run 预览  │      warnings}                 │  3. 配额校验(真实行数)│    Expander (无IO)
 └──────────────┘                                │  4. 单事务落库       │──▶ thing.rs tx 变体
  本体详情页                                    │  5. 名称探测+TOCTOU  │    thing_property/command
  「另存为场景包」─▶ POST export-as-template ────▶│     重试 ≤10        │    alarm_rule.rs
                       (404 防 IDOR)             └────────────────────┘
                          │ 反向导出器 (子树→模板 JSON, 命名泛化启发式)
                          ▼
                   import_export 闭环 (注册为 workspace 模板)
```

## 数据流四路径（实例化）

```
 INPUT ──▶ VALIDATE ──▶ EXPAND ──▶ QUOTA ──▶ PERSIST(单tx) ──▶ OUTPUT
   │          │           │          │            │               │
 nil:      400逐字段   RefNotFound  超限400     中途失败        tree_preview
 scene_name  参数类型/  →400指出   提示用量/   →整体回滚500    【缺口I1c:
 →缺校验I6  min/max    引用名      上限                     名称是解析前的】
 empty:    未知键      500节点护栏                      名称冲突→-N
 参数空→默认 静默I7    环检测400   【TOCTOU:   探测≤10+唯一    【缺口I1b:
 wrong type: 【缺口I3:  深度>5 400   tx外读取,   约束兜底重试    dry-run不探测】
 前端0胁迫C2 event白名单 条件不解析   软限制OV3】  【锁竞争整tx
            虚设】【C1:              重试≤5】
            提交才炸】
```

## Critical 发现（阻塞合并）

| # | 发现 | 位置 | 修法 |
|---|------|------|------|
| C1 | smart_campus 能耗告警 condition 缺 `change_type`/`threshold`/`time_window`，不匹配 `AlarmCondition` serde；dry-run 不解析 condition → 预览正常提交必 400 | `templates/builtin/scenes/smart_campus.json:25`、`seed/system.sql:743`、`scene_instantiator.rs:413` | 改两处 condition 为合法形状；文件校验测试加 condition 反序列化断言；新增真实 seed 端到端实例化测试 |
| C2 | `Number("")===0`，清空参数静默存 0 绕过必填校验 | `web/src/ui/views/marketplace.ts:277-282` | `trim()==="" ? NaN : Number(raw)` |
| C3 | 实例化进行中对话框可经 overlay/×/取消关闭 → 强制跳转或 warnings 静默丢失 | `marketplace.ts:651,656,678` | `closeSceneDialog` 门控 `!submitting`，提交中禁用三处关闭入口 |

## Important 发现（裁定：全部合并前修）

| # | 发现 | Effort |
|---|------|--------|
| I1 | dry-run/落库名称分歧三件套：(a) dry-run 跳过 parent_id 校验（`scene_instantiator.rs:138` 早返在 `:149` 校验前）；(b) dry-run 不做名称探测；(c) 提交响应 tree_preview 用解析前名称（`:199-204` vs `:253-270`）。违反 spec §3.1/V3 与 §7.2；3 个审查维独立命中 | M |
| I2 | `expand_scene_ref` 不合并被引用模板参数默认值，无映射参数必报 InvalidParameter（`scene_template.rs:410-457`） | S |
| I3 | `rule_type:"event"` 在白名单但 `AlarmCondition` 无 Event 变体 → event 规则永不可实例化。裁定： v1 移出白名单 + spec 修订（诚实降级） | S |
| I4 | 预加载阶段 ref 不存在 → `MarketplaceError::Template` → 500，spec §6 要求 400（`scene_instantiator.rs:94,103`；expander 自身的正确 400 路径成死代码） | S |
| I5 | spec §3.5 指标 `scene_instantiations_total` 未实现。裁定： 修 spec 降级为结构化日志 | S（spec） |
| I6 | `scene_name` 零校验（空/空白/换行/超长），垃圾经 `{scene_name}` 传播整树 | S |
| I7 | 未知 `parameter_values` 键静默忽略（`scene_template.rs:500-515` 只迭代已声明参数） | S |
| I8 | 结构化日志缺 §3.5 字段（template_id/耗时/warnings 数/错误类别/引用链）；校验失败早返无日志；handler 把 400 记 error | S |
| I9 | 前端 dry-run 失败零反馈，预览直接消失（配额超限/超 500 全吞） | S |
| I10 | 模板详情加载失败后对话框留空表单可提交（`canSubmit` 不要求 `sceneDetail`） | S |
| I11 | 父本体选择是裸 ID 文本框非 picker。裁定： 本期仅错误可见化，picker 登记 TODOS | S |
| I12 | TOCTOU 唯一约束兜底在 SQLite 不可达（BUSY_SNAPSHOT 先触发）且无测试；测试池无 busy_timeout | S-M |
| I13 | **传递引用不加载**： `collect_refs` 只走根模板树（`scene_instantiator.rs:87`），scene_ref 链 A→B→C 必报误导性 RefNotFound；spec 的环检测 400 经 API 不可达；环检测单测手工构造 map 掩盖了它。E6 组合能力实际只有一层 | M |
| I14 | `param_mapping` 映射值不做目标模板 min/max 校验（`scene_template.rs:436-447`），目标作者的防爆炸护栏失效 | S（随 I2） |

## Minor 处置（裁定）

- **随手修批（22 条）**： 文件校验测试补 condition/rule_type 断言；seed 与 spec §4 偏差（楼栋告警/楼层资源/default_knowledge 两行 NULL）；seed 内嵌 JSON 端到端测试；template_ref 内联解析失败→warning；notification_config 部分配置静默降级→400 或 warning；scene_ref+count/children 组合→400；template_ref 节点自有块被覆盖→拼接或拒绝；localized 缺 name 回退；名称重试耗尽与非名称唯一冲突误标 500→400（仅 Protocol 冲突溢出映射 Validation）；锁检测改 SQLite 扩展错误码；import_wot 补场景包旁路；import 名称竞态（唯一约束续探测+探测错误透传）；导出 NULL category 回退污染→warning；dry-run 配额测试参数化；4 个未覆盖分支断言（event_defs/dashboard、warn-skip、>10 冲突 400、sanitize_label）；CHANGELOG 误归属（终审 Minor 1）；前端 stale 预览清除、导出 warnings 展示、死分页代码删除、N+1 改渐进渲染、预览 warnings 展示、fetchRaw 抽取+`exportSceneTemplate` 改名；`{index}` 静态校验（OV#4）；`count:0` 校验（OV#5）；builtin 文件 vacuous expand 断言改精确断言。
- **登记 TODOS（7 条）**： 父本体 picker（P2/M）、批量 INSERT push_values（P3/M，spec §3.2 偏差记录）、quota tx 内复检或文档化（P3/S）、对话框 a11y（P3/M）、X3 上传 UI（P2/M）、X4 封面图（P3/S）、X5 幂等键（P2/M）。
- **维持既有登记**： catalog 动态/静态分组不一致（TODOS P3，交 health 任务）。

## 扩展裁定（SELECTIVE cherry-pick）

- **采纳**： X1 scene_ref 狗粮（smart_campus 改为引用 smart_building，紧跟 I13 修复验证 E6）；X2 dry-run 预览显示配额用量。
- **登记 TODOS**： X3/X4/X5（见上）。
- **维持跳过**： E1 模拟数据流、E3 人设继承（2026-08-31 已裁定）。

## 失败模式登记（审查后残余）

```
  CODEPATH                    | FAILURE MODE              | RESCUED | TEST | USER SEES | LOGGED
  ----------------------------|---------------------------|---------|------|-----------|--------
  instantiate commit          | 内置包 condition 失配(C1)  | 修复后Y | 修复后Y | 400→不再发生 | 修复后Y
  参数输入                    | 清空→0 (C2)               | 修复后Y | 手测   | 校验错误    | n/a
  实例化中关闭对话框 (C3)      | 提交继续, nav劫持          | 修复后Y | 手测   | 提交中禁关  | n/a
  quota 检查                  | 并发双过 (OV3)            | N(接受) | N    | 静默超限    | Y(警告)  ← 已知残余, TODOS 文档化
  名称探测                    | DB 错误误标 400           | 修复后Y | Y    | 正确 500   | Y
  template_ref 内联           | 引用模板 JSON 损坏         | 修复后Y | Y    | warning    | Y
  mqtt telemetry 路由         | 升级窗口旧形状 buffer      | N(接受) | Y    | 静默丢弃    | N       ← 终审 Minor 3, 维持
  discover 落库               | 无幂等 (do_scan stub 期   | N(接受) | N    | 主键冲突    | Y       ← TODOS P2, 当前不可触发
                              |  间不可触发)              |         |      |            |
```

修复前 CRITICAL GAP 3 条（C1-C3），修复后 0；残余静默路径均为已登记的接受项。

## 部署与回滚

```
 部署序列: 迁移 20260831000001 (UPDATE-only, 无 DDL, 已验证) → cloud 部署 → edge 同步升级 (CHANGELOG 已声明 co-upgrade)
 回滚: git revert + 重部署; 无新表新列 → 无需迁移回滚; 实例化产物是用户数据不回滚
 风险窗口: edge/cloud 版本错配期遥测静默丢弃 (终审 Minor 3, 已接受)
```

## 长期轨迹

- 可逆性： 4/5（零新表零新列；唯一硬承诺是模板 JSON schema 的向后兼容）
- 债务： linked_data 三块（knowledge/event_defs/dashboard）仅记录未消费——待 Thing Agent 配置演进接入；spec §8 非目标清单清晰
- 门外声音战略点（OV#8）已消化： 投机子系统（scene_ref/导出）先于"内置包端到端可实例化"建成——本次修复包的 T1/T5/T14 把端到端验证补齐，教训记入 learnings

## Implementation Tasks

- [ ] **T1 (P1, human: ~半天 / CC: ~20min)** — seed+builtin — C1: 修 smart_campus condition 形状（JSON+SQL 两处）；文件校验测试加 condition/rule_type 反序列化断言；新增真实 seed smart_campus 端到端实例化测试
  - Files: `templates/builtin/scenes/smart_campus.json`, `crates/db/src/seed/system.sql`, `crates/db/tests/scene_templates_file_test.rs`, `apps/cloud/src/tests/scene_instantiate_test.rs`
  - Verify: `cargo test -p db --test scene_templates_file_test && cargo test -p tinyiothub-cloud scene_instantiate`
- [ ] **T2 (P1, CC: ~5min)** — frontend — C2: 空参数→NaN
  - Files: `web/src/ui/views/marketplace.ts` | Verify: 手测清空参数字段出校验错误
- [ ] **T3 (P1, CC: ~5min)** — frontend — C3: 提交中禁关对话框
  - Files: `web/src/ui/views/marketplace.ts` | Verify: 手测提交中 overlay/×/取消禁用
- [ ] **T4 (P1, human: ~1天 / CC: ~30min)** — instantiator — I1: dry-run 前移 parent 校验+只读名称探测；提交响应 tree_preview 用解析后名称重建；§7.2 parity 测试
  - Files: `apps/cloud/src/domains/marketplace/scene_instantiator.rs`, `apps/cloud/src/tests/scene_instantiate_test.rs`
- [ ] **T5 (P1, human: ~1天 / CC: ~30min)** — expander — I13+I2+I14: collect_refs 传递加载（BFS+visited）；scene_ref 目标默认值合并；映射值 min/max 校验；链/环/默认值/越界测试
  - Files: `apps/cloud/src/domains/marketplace/scene_instantiator.rs`, `crates/db/src/scene_template.rs`
- [ ] **T6 (P1, CC: ~10min)** — expander+spec — I3: event 移出白名单 + spec §2.2 修订；I4: ref 预加载错误→Validation 400
  - Files: `crates/db/src/scene_template.rs`, `apps/cloud/src/domains/marketplace/scene_instantiator.rs`, `docs/superpowers/specs/2026-08-31-scene-template-design.md`
- [ ] **T7 (P1, CC: ~15min)** — API 校验 — I6 scene_name 校验 + I7 未知参数键 400 + I9/I10 前端错误态
  - Files: `apps/cloud/src/domains/marketplace/handler.rs`, `crates/db/src/scene_template.rs`, `web/src/ui/views/marketplace.ts`
- [ ] **T8 (P1, CC: ~20min)** — tests — I12: TOCTOU 重试故障注入单测 + 测试池 busy_timeout
  - Files: `apps/cloud/src/tests/scene_instantiate_test.rs`, `apps/cloud/src/test_utils.rs`
- [ ] **T9 (P2, CC: ~20min)** — observability — I8+I5: 日志字段补齐（template_id/耗时/warnings/类别/引用链）+ spec §3.5 修订（指标降级）
  - Files: `apps/cloud/src/domains/marketplace/scene_instantiator.rs`, handler.rs, spec
- [ ] **T10 (P2, human: ~2天 / CC: ~60min)** — minor 随手批（22 条，清单见上文）
- [ ] **T11 (P2, CC: ~30min)** — 扩展 — X1 scene_ref 狗粮（smart_campus→scene_ref smart_building）+ X2 dry-run 配额用量显示
  - Files: `templates/builtin/scenes/smart_campus.json`, `crates/db/src/seed/system.sql`, `scene_instantiator.rs`, `web/src/ui/views/marketplace.ts`
- [ ] **T12 (P2, CC: ~15min)** — TODOS.md 登记 7 条延迟项 + spec §3.2/§6/§7 修订（批量 INSERT 偏差、TOCTOU 说明）
- [ ] **T13 (P1, CC: ~10min)** — 全门禁重跑： `cargo test --workspace` + `cargo clippy --workspace -- -D warnings`

## NOT in scope（明确排除）

- E1 模拟数据流、E3 人设继承（前审裁定跳过）
- 用户自制场景包上传 UI、封面图、幂等键（X3/X4/X5 → TODOS）
- 批量 INSERT push_values、quota tx 内复检、picker、a11y（→ TODOS）
- 3D 场景视图、资源真实托管、跨节点告警引用、模板版本升级推送（spec §8 非目标）

## What already exists（复用确认）

ThingTemplate 单表/device_info JSON 存储、import_export 闭环、ThingTemplateInstaller 并列模式、count_things_by_workspace 真实行数、busy_timeout 全局、确定性 workspace→builtin 查找（ORDER BY + 测试钉住）、is_composition() 六调用点门（T1 裁定已落实）。

## Dream state delta

本 PR + 修复包落地后： 内容供给侧闭环完整（内置包可实例化 ✓、导出→编辑→再注册 ✓、组合引用真实可用 ✓）。距 12 个月理想的剩余缺口： 模拟数据流（E1）、社区上传 UI（X3）、模板版本演进、3D 视图。
