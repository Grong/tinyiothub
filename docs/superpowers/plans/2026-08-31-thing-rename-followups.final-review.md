# Final Whole-Branch Review — thing-rename follow-ups (PR-3)

- Range: `a001c525..8519664a` (10 commits; `6f9d64bd` scene-template docs 已确认 docs-only 并排除)
- Reviewer: final broad pass（任务级评审已全部通过；本审聚焦跨任务接缝与端到端契约）
- Gates: workspace 全绿 / clippy / fmt 已验证，未重跑

## Overall Verdict: CLEAN TO MERGE

无 Critical / Important 发现。4 条 Minor（1 条建议合并前顺手修，3 条可 ship-as-is）。ledger 7 条 deferred minors 全部维持 deferred（分诊见下）。

---

## (a) MQTT 契约端到端终判 — 通过

**Discovery 链**（逐字段核对两侧源码，非仅 diff）：

- edge 发布 topic：`apps/edge/src/modules/gateway/service.rs` `discovery_topic()` → `{prefix}/thing/discover` = `tinyiothub/{ws}/gateway/{gw}/thing/discover`（6 段）。cloud 订阅 `mqtt_client.rs:83`（通配）与 `:340`（per-gateway）逐字一致。
- cloud 守卫 `mqtt_client.rs:199`：`parts.len() >= 6 && parts[5] == "discover"`；`parts.len() >= 6` 保证 `parts[5]` 索引安全。下一臂 `:208` `parts.len() >= 7 && parts[5] != "discover"`。**两臂互斥无死区**：6 段 discover 命中第一臂；7 段 thing telemetry 命中第二臂；6 段 `thing/{非discover}` 落入 `_ => None`，但 edge 从不发布该形状（thing telemetry 恒 7 段），无实际死区。7+ 段 `thing/discover/...` 归第一臂（解析失败即丢弃），与改动前行为等价（旧守卫下同样 drop），非回归。
- payload：edge `DiscoveredThing{name, category, protocol_type, address, driver_name, driver_options}`（`apps/edge/src/modules/gateway/types.rs`，snake_case）与 cloud `DiscoveredThing`（`apps/cloud/src/domains/driver/gateway/types.rs:79-86`）**逐字段一致**；`type:"thing_discover"` 两侧同。cloud `handle_thing_discover`（`gateway/service.rs:240`）`name` 必填映射 `CreateThingRequest.name`，edge 以 scan_all 返回的 ID 填 `name`——链路易通。
- 幂等性：`create_things_batch` 纯 INSERT，重复 discover 会主键冲突——但 `do_scan`（`apps/edge/src/modules/driver/service.rs:74`）是 stub 恒返回空，`main.rs` 的 `!discovered.is_empty()` 守卫使该路径当前不可触发；已记录 TODOS P2。**ship-as-is**。

**Telemetry 链**：

- edge `build_telemetry_payload` 产出 `{"type":"telemetry","data":[...],"timestamp":<unix>}`，与 cloud `TelemetryMessage{msg_type(rename "type"), data: Value, timestamp: i64}`（`types.rs:91-96`）逐字段对齐。cloud 侧回归测试 `gateway_telemetry_edge_payload_routes` 用 edge 真实形状（data 为 ID 字符串数组）走 `route_data_message` 全路径，断言到位。
- **离线 buffer 重放形状一致**：buffer 存的是同一份 payload 字节 + 同一 topic（`{prefix}/telemetry`），`flush_batch_with → publish_raw` 原样重放，无需另改 ✓。
- 部署耦合：CHANGELOG 已声明 edge/cloud 须同步升级 ✓。

## (b) 迁移安全性 — 通过

- `default_agent_config`（`crates/agent/src/config.rs:131`）实际 JSON 顶层含 `tool_denylist: ["delete_thing","delete_schedule"]`（字符串数组）。迁移 `20260831000001` 的 `replace(config, '"delete_device"', '"delete_thing"')` 为纯文本带引号 token 替换，嵌套层级无关——只要旧名以完整 JSON 字符串 token 出现即命中；`'"get_device"'` 含收尾引号，不误伤 `"get_devices"` 类前缀（测试 `agent_prefix` 的 `"delete_device_extra"` 钉住）。WHERE LIKE 与 REPLACE 命中条件一致。
- 迁移链共存：`20260831000001` 为最新序号，排在 `20260828000001`（同为 replace() 模式但作用于 policy 三表，无表重叠）之后；`sqlx::migrate!("./migrations")` 编译期嵌入，门禁全绿证明已生效。旧默认 denylist（81d073fc 前的 `"delete_device"`）场景被迁移测试 `agent_old` 样本覆盖。
- 基线化后（0.5.0.0）的新库 WHERE 全不命中，无-op ✓。

## (c) 跨任务接缝 — 通过

- Task 1 discovery 接线与 Task 2 telemetry 共享 `scan_all -> Vec<String>`（thing ID 列表）语义假设，两处一致；均为占位语义，TODOS P2/P3 已登记。
- Task 4 类型改名与 Task 5 小修同触 `mqtt_client.rs`（守卫+测试 vs 局部变量 `device_telemetry→thing_telemetry`）与 `pool_adapter.rs`，最终态无冲突，gates 绿。
- Task 4 核验：`dbdb5660` 仅 5 文件、26/26 行纯类型名替换；serde 字段行（`device_ids`/`device_types`/`device_filter` 等 wire 键）零变化 ✓。

## (d) 行为变化清单 vs CHANGELOG — 基本完整

| 行为变化 | CHANGELOG 条目 |
|---|---|
| discovery guard off-by-one + edge topic/payload + 接线落库 | ✓ Fixed 第 1 条 |
| telemetry payload 形状 + 同步升级警示 | ✓ Fixed 第 2 条 |
| catalog 4 工具 id + group id `device→thing` + 迁移 20260831000001 | ✓ Fixed 第 3 条（group id 未单列，并入工具 id 条目，可接受） |
| tag stats `by_type.device`→`thing` | ✓ Fixed 第 4 条 |
| tombstone msg `/api/v1` 前缀 | ✓ 同上条 |
| pairing ack 缺 `thing_id` fail fast | ✓ Fixed 第 5 条 |
| 6 个 Device* 类型改名 | ✓ Unreleased 列表条目（位置偏差见 deferred 分诊） |

## Findings

### Minor 1（建议合并前顺手修）— CHANGELOG 误归属 marketplace DeviceInfo 改名
- **File**: `CHANGELOG.md`（PR-3 Device* 条目，diff 行 57）
- **事实**：本分支 `dbdb5660` 不含任何 marketplace/template 文件；base `a001c525` 上 `apps/cloud/src` 已无 `DeviceInfo`（git grep 为空）。`DeviceInfo→ThingInfo` 实际发生在 PR-2 `2b93b845`（`domains/thing/template/{types,exporter}.rs`）。
- **Failure scenario**: 读者按 CHANGELOG 在 PR-3 范围内找 marketplace 改名找不到；归因混乱，无功能影响。
- **处置**：删除该短语或改为"marketplace 侧 DeviceInfo 已于 PR-2 改名"。一句话修复，建议合并前顺手；不改也不阻断。

### Minor 2（ship-as-is）— DiscoveredThing 字段级跨 crate 契约无测试钉住
- **File**: `apps/cloud/src/shared/mqtt_client.rs:357`（discover 测试用 `"things": []`）；`apps/edge/src/modules/gateway/service.rs` 测试仅断言 topic 字符串
- **Failure scenario**: 任一侧未来给 `DiscoveredThing` 加/改字段（如把 `name` 改 `thing_id`），两侧测试仍全绿，运行时 cloud `.ok()` 静默丢弃整个 discover 消息——回到本 PR 修的那类死链。当前两侧字段已逐字核对一致，属防漂移缺口而非现行 bug。
- **处置**：可记入 TODOS（建议并入既有"discovery e2e 验证"P2 条目），不阻断。

### Minor 3（ship-as-is，残余风险）— 升级窗口期 edge 离线 buffer 旧形状 payload 被新 cloud 静默丢弃
- **File**: `apps/edge/src/modules/telemetry/service.rs`（buffer 写入）+ `apps/cloud/src/shared/mqtt_client.rs:185`（`.ok()` 静默 drop）
- **Failure scenario**: edge 升级前 buffer 里积有旧形状（裸 JSON 数组）telemetry；edge/cloud 均升级后重放，cloud `TelemetryMessage` 解析失败走 `.ok()` → None，无告警即丢数据。仅影响升级窗口期的积压 buffer，量小且 CHANGELOG 已声明同步升级。
- **处置**：接受；如需可在后续给 route 失败加 warn 日志（非本 PR 范围）。

### Minor 4（ship-as-is，残余风险）— `tags.tag_type` 无 'device'→'thing' 归一化
- **File**: `apps/cloud/src/domains/thing/tag/handler.rs:297`（stats 匹配臂）与 `:131` `create_tag`（`request.tag_type` 自由文本直存）
- **Failure scenario**: 用户经 API 显式创建 `type:"device"` 的 tag，stats 两桶均不计入（改动前计入 device 桶）。PR-2 的归一化只覆盖 `tag_bindings.target_type`，不覆盖 `tags.tag_type`；0.5.0.0 基线重置后无存量数据问题，仅新写入的边缘情形。
- **处置**：接受（前端无消费、自由文本语义）；如日后收紧 tag_type 枚举自然消解。

## Deferred-minor 分诊结论（ledger 7 条）

| # | 条目 | 结论 | 理由 |
|---|---|---|---|
| 1 | handle_thing_discover 无幂等 | **ship-as-is** | do_scan stub 恒空 + `is_empty` 守卫使路径不可触发；TODOS P2 已登记，driver runtime 落地时必修 |
| 2 | DiscoveredThing 信息贫乏 | **ship-as-is** | 同根因（scan_all 只回 ID）；TODOS P2 |
| 3 | discovery 无真实数据流过 / 缺 e2e | **ship-as-is** | 需真实 broker 环境；契约两侧已逐字段核对 + 单测钉住路由与形状；建议把 Finding Minor 2 并入该 TODOS 条目 |
| 4 | telemetry data 元素语义占位 | **ship-as-is** | 契约形状已对齐，内容语义待 driver runtime；TODOS P3 |
| 5 | 动态 catalog 分组 `("device","设备管理")` vs 静态 `"thing"` 不一致 | **ship-as-is** | 仅 UI 分组显示抖动；TODOS P3（交 health 任务） |
| 6 | CHANGELOG Device* 条目未放 `### Changed` 子节 | **ship-as-is** | 纯结构 cosmetic；内容本身在 Unreleased 列表中可读。若顺手修 Minor 1 可一并调整 |
| 7 | TODOS permission.rs 死列措辞精度 | **ship-as-is** | 文档措辞，无误导性错误 |

## 测试盲区（仅增量）

1. **Finding Minor 2**：discover 链路缺字段级 payload round-trip 测试（cloud 用空数组、edge 只测 topic）。
2. discovery 发布为启动一次性、失败仅 warn 不重试（telemetry 每 tick 重试，discovery 无等效机制）；do_scan stub 期间无实际影响，driver runtime 落地时需一并考虑——已在 TODOS P2 幂等条目覆盖范围内，不另列 finding。
3. 迁移测试未走真实 `default_agent_config` 序列化产物做样本（用代表性手写 JSON）；`replace()` 语义与层级无关，风险可忽略。

## Residual Risks 汇总

- edge/cloud 必须同步升级（CHANGELOG 已声明）：旧 edge 发 `{prefix}/discovery` + 裸数组 telemetry 会被新 cloud 静默丢弃；反之新 edge 对旧 cloud 同样不通。这是既定 co-upgrade 契约，非缺陷。
- 升级窗口期离线 buffer 旧形状数据静默丢失（Minor 3）。
- do_scan stub 使 discovery 链路当前零真实流量，本 PR 的正确性由契约核对 + 单测保证，真实 broker e2e 仍欠（TODOS P2）。
