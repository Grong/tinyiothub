# device→thing 遗留问题修复实施计划(PR-3)

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 修复改名终审遗留的问题:MQTT discovery 端到端死链(含 off-by-one)、telemetry payload 形状不匹配、agent tools catalog 陈旧工具名、apps 残留 Device\* 类型、一批小修与测试缺口。

**Architecture:** 两项真实 bug(discovery/telemetry)按"cloud 契约为准、edge 对齐"原则修复并补契约测试;catalog/残留类型为机械改名;小修合并为一个任务。严格 TDD：每个 bug 先写复现/契约测试。

**Tech Stack:** Rust + tokio + rumqttc(MQTT)、sqlx(SQLite)、axum。

## Global Constraints

- **分支**：从 `refactor/thing-rename-pr2`(HEAD `a001c525`)切出 `refactor/thing-followups`;PR stacked,base 为 `refactor/thing-rename-pr2`。
- **CLAUDE.md 铁律**：不删除已有 dead code——`crates/db/src/permission.rs` 死列查询函数、web `thing-cache.updateProperty`、`crates/runtime/src/query_service.rs` 孤儿文件**只记录不动**(TODOS.md)。
- **crate/lib 名**:package `db` → lib `tinyiothub_storage`;cloud package `tinyiothub-cloud`;edge package `tinyiothub-edge`;agent crate 名以 `crates/agent/Cargo.toml` 为准。
- **MQTT 契约终态(cloud 为准,verbatim)**:
  - discovery topic:`tinyiothub/{workspace_id}/gateway/{gateway_id}/thing/discover`(6 段）
  - discovery payload:`ThingDiscoverMessage`(snake_case)`{"type": "...", "things": [DiscoveredThing...]}`(DiscoveredThing 含 `thing_id`/`category` 等，PR-2 已改名）
  - gateway telemetry topic:`tinyiothub/{ws}/gateway/{gw}/telemetry`,payload `TelemetryMessage`(snake_case)`{"type": "...", "data": <array>, "timestamp": <i64>}`
  - cloud 路由守卫修正:discover 分支 `parts.len() >= 6 && parts[5] == "discover"`(6 段）;thing telemetry 分支保持 `parts.len() >= 7`
- **catalog 终态**:group `id: "thing"`；工具 id/name:`search_things`/`get_thing`/`create_thing`/`delete_thing`（对齐 MCP 注册名，见 `apps/cloud/src/domains/mcp/tools/thing.rs:117/582/737/828`);`read_properties`/`write_properties`/`send_command` 不变。
- **agent_tools 持久化**:`tool_overrides` 列为 JSON `{"enabled": [...], "disabled": []}`。若其中存的是旧工具名，需新迁移 `crates/db/migrations/20260831000001_tool_override_rename.sql` 用带引号 token 的 `replace()` 翻转（参照 `20260828000001` 的模式）；若调查发现存的是别的标识，按调查结论处理并报告。
- **残留类型映射**:`DeviceFilterRequest`→`ThingFilterRequest`、`DeviceOnlineStatus`→`ThingOnlineStatus`、`DeviceSnapshot`→`ThingSnapshot`、`DeviceNotFound`→`ThingNotFound`、`DeviceCacheAdapter`→`ThingCacheAdapter`、marketplace 侧 `DeviceInfo`→`ThingInfo`。**仅类型名**；serde 字段名不动（`device_ids`/`device_name` 等 alarm/notify wire 键刻意保留）。
- **保留不动**:`EventType::Device` 变体与 `ThingEventType::DeviceAlarm` 等子变体（serde wire 值）;`events.source_type/actor='device'`;db `ThingStats` 4 字段；pool_adapter 4 键回退链；a2ui 前端 Device\* 别名兜底。
- **门禁**：每任务 `cargo test -p <crate>` 绿；收尾 `cargo test --workspace` 全绿 + clippy `-D warnings` + fmt clean。
- zsh:`grep -rln ... | xargs perl -pi -e '...'`；新迁移文件后 `touch crates/db/src/migrations.rs`(sqlx::migrate! 嵌入重建陷阱）。

---

### Task 1: MQTT discovery 端到端修复

**Files:**
- Modify: `apps/cloud/src/shared/mqtt_client.rs`(discover 分支守卫 off-by-one，约 :200)
- Modify: `apps/edge/src/modules/gateway/service.rs:157-162`(`publish_discovery` topic `{prefix}/discovery`→`{prefix}/thing/discover`)
- Modify: `apps/edge/src/modules/gateway/pairing.rs` 或 edge 启动流程（pairing 成功后接入 discovery 发布——见 Step 3 调查规则）
- Test: `apps/cloud/src/shared/mqtt_client.rs` 的 tests 模块（若无则新建内联 `#[cfg(test)]`)

**Interfaces:**
- Consumes: `ThingDiscoverMessage`/`DiscoveredThing`(cloud `domains/driver/gateway/types.rs:71`);edge `GatewayService::topic_prefix()`(`service.rs:108`)。
- Produces: cloud `route_data_message` 能解析 6 段 discover topic;edge 发布后 cloud 可收到。

- [ ] **Step 1: 写失败的 cloud 路由契约测试**

在 `mqtt_client.rs` 内联 tests 模块（参照文件内既有测试模式；若无测试模块，新建 `#[cfg(test)] mod tests`）写：

```rust
#[test]
fn discover_topic_six_segments_routes() {
    // tinyiothub/{ws}/gateway/{gw}/thing/discover — 6 段
    let payload = serde_json::to_vec(&serde_json::json!({
        "type": "thing_discover",
        "things": []
    })).unwrap();
    let msg = route_data_message("tinyiothub/ws1/gateway/gw1/thing/discover", &payload);
    assert!(matches!(msg, Some(GatewayDataMessage::ThingDiscover { .. })),
            "6-segment discover topic must route, got {:?}", msg);
}

#[test]
fn thing_telemetry_seven_segments_still_routes() {
    let payload = serde_json::to_vec(&serde_json::json!({
        "type": "thing_telemetry", "thing_id": "t1", "data": {}, "timestamp": 0
    })).unwrap();
    let msg = route_data_message("tinyiothub/ws1/gateway/gw1/thing/t1/telemetry", &payload);
    assert!(matches!(msg, Some(GatewayDataMessage::ThingTelemetry { .. })));
}
```

（函数名/类型名以文件内实际为准——先读 `route_data_message` 签名与 `ThingTelemetryMessage` 字段调整 payload 样例。)

- [ ] **Step 2: 运行确认失败 → 修守卫**

Run: `cargo test -p tinyiothub-cloud mqtt_client`
Expected: 第一个测试 FAIL（守卫 `parts.len() >= 7` 丢弃 6 段 topic)

修 `mqtt_client.rs:200` 附近：discover 分支改 `parts.len() >= 6 && parts[5] == "discover"`；telemetry 分支保持 `parts.len() >= 7 && parts[5] != "discover"`（顺序与互斥保持不变）。再跑测试转绿。

- [ ] **Step 3: edge 侧对齐 + 接入**

1. `service.rs:157-162` `publish_discovery`:`format!("{}/discovery", ...)` → `format!("{}/thing/discovery")`…不对——终态是 `thing/discover`:`format!("{}/thing/discover", self.topic_prefix())`。
2. payload 对齐：找 `publish_discovery` 的调用点（终审确认当前**零调用方**=dead path)。调查规则：
   - 读 `apps/edge/src/modules/gateway/pairing.rs` 的 `run_pairing` 返回后调用方（edge main/bootstrap，可能在 `apps/edge/src/main.rs` 或 `modules/mod.rs`),确定 pairing 成功 + driver service 就绪后的位置；
   - 读 cloud `GatewayDataMessage::ThingDiscover` 的处理路径（`domains/driver/gateway/service.rs`），确认 cloud 收到后**有实际处理**（注册 thing 等）。若 cloud 侧仅解析后丢弃（无 handler），则**不接 wiring**（发了也白发），只修 contract 对齐 + 在 TODOS.md 记录"discovery 链路两端已对齐但 cloud 无 handler，待接线"；若有 handler，在 edge bootstrap 的 pairing 成功后加一次性发布：`let things = driver_service.scan_all().await?; let msg = ThingDiscoverMessage { msg_type: "thing_discover".into(), things }; gateway.publish_discovery(&serde_json::to_vec(&msg)?).await?;`(DiscoveredThing 的构造字段以 cloud `types.rs` 实际为准；edge 侧若无该类型，在 edge `gateway/types.rs` 定义同形结构）。
3. edge 侧测试：为 topic 拼接写单测（`publish_discovery` 的 topic 参数断言，可用 mock 或直接测试 format 结果——若 service 难 mock，抽一个 `fn discovery_topic(&self) -> String` 纯函数并对它测试）。

- [ ] **Step 4: 验证 + Commit**

```bash
cargo test -p tinyiothub-cloud mqtt_client && cargo test -p tinyiothub-edge
git add -A && git commit -m "fix(mqtt): repair gateway discovery path end-to-end (6-segment guard + edge topic/payload)"
```

---

### Task 2: telemetry payload 形状对齐

**Files:**
- Modify: `apps/edge/src/modules/telemetry/service.rs:28-35`(`collect_and_forward` payload 构造）
- Test: 同文件内联 tests（payload 形状契约）

**Interfaces:**
- Consumes: cloud `TelemetryMessage { msg_type("type"), data, timestamp }`(`apps/cloud/src/domains/driver/gateway/types.rs:91`,snake_case,`type` rename)。
- Produces: edge 发布 `{"type":"telemetry","data":[...],"timestamp":<unix>}`;offline buffer 重放路径同形状（同一 payload 字节，无需另改）。

- [ ] **Step 1: 写失败测试**

在 `telemetry/service.rs` 内联 tests:

```rust
#[test]
fn telemetry_payload_matches_cloud_contract() {
    let things = vec![serde_json::json!({"thing_id": "t1", "value": 42})];
    let payload = build_telemetry_payload(things.clone()); // 待抽取的纯函数
    let parsed: serde_json::Value = serde_json::from_slice(&payload).unwrap();
    assert_eq!(parsed["type"], "telemetry");
    assert!(parsed["data"].is_array());
    assert!(parsed["timestamp"].is_i64());
    // cloud 侧反序列化形态(结构等价):字段名 type/data/timestamp 全在
}
```

- [ ] **Step 2: 运行确认失败 → 实现**

Run: `cargo test -p tinyiothub-edge telemetry` → FAIL（函数不存在/形状不对）。

实现：在 `telemetry/service.rs` 抽纯函数 `fn build_telemetry_payload(things: Vec<serde_json::Value>) -> Vec<u8>`,`collect_and_forward` 改用它：

```rust
fn build_telemetry_payload(things: Vec<serde_json::Value>) -> Vec<u8> {
    serde_json::to_vec(&serde_json::json!({
        "type": "telemetry",
        "data": things,
        "timestamp": chrono::Utc::now().timestamp(),
    })).expect("telemetry payload serialization is infallible for Value")
}
```

（`scan_all` 的返回类型若非 `Vec<Value>`，先 `serde_json::to_value` 转换；chrono 不可用就用 `std::time::SystemTime`。)

- [ ] **Step 3: cloud 侧解析回归**

`cargo test -p tinyiothub-cloud mqtt_client`(cloud 对 `TelemetryMessage` 的解析测试若不存在，在 mqtt_client tests 加一条：用 Step 1 的样例 payload 走 `route_data_message("tinyiothub/ws1/gateway/gw1/telemetry", ...)`,断言 `Some(GatewayDataMessage::Telemetry{..})`)。

- [ ] **Step 4: Commit**

```bash
cargo test -p tinyiothub-edge && cargo test -p tinyiothub-cloud mqtt_client
git add -A && git commit -m "fix(edge): wrap telemetry payload in TelemetryMessage contract shape"
```

---

### Task 3: agent tools catalog 工具名更新

**Files:**
- Modify: `crates/agent/src/tools/catalog.rs:155-175`(group id + 4 个工具 id/name)
- Modify: `crates/db/src/agent.rs`(若 tool_overrides 存旧名，迁移外的读取侧兼容）
- Create（条件）: `crates/db/migrations/20260831000001_tool_override_rename.sql`
- Test: catalog 测试 + 条件性迁移测试

**Interfaces:**
- Consumes: MCP 注册名（`mcp/tools/thing.rs`);`/tools/toggle` handler(`apps/cloud/src/domains/agent/host/tools/handler.rs`)。
- Produces: catalog id 与 MCP 注册名一致；存量 tool_overrides 旧名不失效。

- [ ] **Step 1: 调查（报告必须回答）**
  1. 读 `tools/handler.rs` 的 toggle 实现与 `crates/db/src/agent.rs` 的 tool_overrides 读写：toggle 存的标识是 catalog id 还是 MCP 注册名？（两者原同为 `search_devices` 等，现已分叉。)
  2. effective-tools 计算（`/tools/effective`）如何匹配 toggle 记录与 MCP 工具？
  3. 结论：catalog id 应与哪侧对齐（应为 MCP 注册名），tool_overrides 存量行是否含旧名（seed/demo 与迁移 grep `search_devices` 全仓确认）。

- [ ] **Step 2: 写失败测试**

catalog 测试（catalog.rs 内联或既有测试文件）:

```rust
#[test]
fn catalog_tool_ids_match_mcp_registry_names() {
    let catalog = build_tools_catalog_json();
    let ids: Vec<&str> = catalog["groups"].as_array().unwrap().iter()
        .flat_map(|g| g["tools"].as_array().unwrap().iter()
            .map(|t| t["id"].as_str().unwrap()))
        .collect();
    for expected in ["search_things", "get_thing", "create_thing", "delete_thing",
                     "read_properties", "write_properties", "send_command"] {
        assert!(ids.contains(&expected), "catalog missing MCP name {expected}");
    }
    assert!(!ids.contains(&"search_devices"), "stale name leaked");
}
```

- [ ] **Step 3: 实现**

catalog.rs:group `id: "device"`→`"thing"`(label 文案不动）;4 个工具的 `id`/`name` 双字段改 thing 系（`read_properties`/`write_properties`/`send_command` 不动）。若 Step 1 发现 tool_overrides 存量含旧名：加迁移 `20260831000001_tool_override_rename.sql`（带引号 token replace，参照 20260828000001)+ 迁移测试（参照 policy_action_rename_tests.rs 模式）。`touch crates/db/src/migrations.rs`。

- [ ] **Step 4: 验证 + Commit**

```bash
cargo test -p tinyiothub-agent && cargo test -p db && cargo test -p tinyiothub-cloud tools
git add -A && git commit -m "fix(agent): align tools catalog ids with MCP registry names (+ tool_overrides migration)"
```

---

### Task 4: apps 残留 Device\* 类型改名

**Files:**
- Modify: `apps/cloud/src/domains/notify/dto.rs`(DeviceFilterRequest)、`apps/cloud/src/domains/admin/thing/monitoring.rs:24`(DeviceOnlineStatus)、`apps/cloud/src/domains/agent/host/types.rs`(DeviceSnapshot/DeviceNotFound)、`apps/cloud/src/shared/runtime_ports.rs`(DeviceCacheAdapter)、marketplace DeviceInfo 所在文件（先 grep 定位）及全部引用点

**Interfaces:**
- Produces: `ThingFilterRequest`/`ThingOnlineStatus`/`ThingSnapshot`/`ThingNotFound`/`ThingCacheAdapter`/`ThingInfo`(marketplace 局部）。

- [ ] **Step 1: 定位 + 改名**

```bash
grep -rn "\bDeviceFilterRequest\b\|\bDeviceOnlineStatus\b\|\bDeviceSnapshot\b\|\bDeviceNotFound\b\|\bDeviceCacheAdapter\b" apps/cloud/src --include="*.rs" | cut -d: -f1 | sort -u
grep -rn "\bDeviceInfo\b" apps/cloud/src/domains/marketplace --include="*.rs" | head -5
grep -rln "\bDeviceFilterRequest\b\|\bDeviceOnlineStatus\b\|\bDeviceSnapshot\b\|\bDeviceNotFound\b\|\bDeviceCacheAdapter\b" apps/cloud/src --include="*.rs" | xargs perl -pi -e '
s/\bDeviceFilterRequest\b/ThingFilterRequest/g;
s/\bDeviceOnlineStatus\b/ThingOnlineStatus/g;
s/\bDeviceSnapshot\b/ThingSnapshot/g;
s/\bDeviceNotFound\b/ThingNotFound/g;
s/\bDeviceCacheAdapter\b/ThingCacheAdapter/g;
'
# marketplace DeviceInfo 单独处理(可能与 core ThingInfo 撞名,若在 marketplace 模块内局部则用 ThingInfo,撞名时用 MktThingInfo 并在报告说明)
```

- [ ] **Step 2: 编译修正 + 验证 serde 字段不变**

```bash
cargo check -p tinyiothub-cloud 2>&1 | grep -E "^error" | head -10
cargo test -p tinyiothub-cloud
```

核验规则：这些类型的 **serde 字段名/JSON 键不得变化**（只改 Rust 类型名）；对 `ThingFilterRequest` 等带 Serialize/Deserialize 的类型，diff 中确认无字段行变化。

- [ ] **Step 3: Commit**

```bash
git add -A apps/cloud && git commit -m "refactor(cloud): rename residual Device* types to Thing* (type names only, wire keys unchanged)"
```

---

### Task 5: 小修一批（tag stats + tombstone 文案 + 命名卫生）

**Files:**
- Modify: `apps/cloud/src/domains/thing/tag/handler.rs:296-310`(get_tag_stats)
- Modify: `apps/cloud/src/domains/admin/thing/management.rs:14`(tombstone msg 文案）
- Modify: `apps/cloud/src/domains/agent/host/pool_adapter.rs`（测试名 5 处）、`apps/cloud/src/shared/mqtt_client.rs:341`（局部变量 `device_telemetry`)
- Test: tombstone 文案断言更新（Task 7-of-PR-2 加的 410 测试）、tag stats 测试

**Interfaces:**
- Produces: tag stats `by_type` 含 `thing`/`app` 键（`device` 键移除——原为恒 0 死桶）;tombstone msg `/api/v1/devices ... /api/v1/things`。

- [ ] **Step 1: 写失败测试**

tag stats 测试（tag handler 测试文件，参照 `apps/cloud/src/tests/tag_handler_tests.rs` 模式）：创建 type=thing ×2 + type=app ×1，调 stats 端点，断言 `by_type.thing == 2 && by_type.app == 1` 且 JSON 无 `device` 键。410 测试的 msg 断言更新为 `/api/v1/devices has been removed. Use /api/v1/things instead.`（先红）。

- [ ] **Step 2: 实现**

1. get_tag_stats:`"device" => device_count` 分支改 `"thing" => thing_count`,`by_type` JSON 键 `device`→`thing`。
2. management.rs msg 文案改 `/api/v1/devices has been removed. Use /api/v1/things instead.`。
3. pool_adapter.rs 测试名 5 处 `*_device_id_*`→`*_thing_id_*`（语义对齐现断言）。
4. mqtt_client.rs:341 `device_telemetry`→`thing_telemetry`（局部变量）。

- [ ] **Step 3: 验证 + Commit**

```bash
cargo test -p tinyiothub-cloud tag && cargo test -p tinyiothub-cloud tombstone 2>/dev/null; cargo test -p tinyiothub-cloud
git add -A && git commit -m "fix(cloud): tag stats thing bucket + tombstone path wording + naming hygiene"
```

---

### Task 6: edge pairing ack 解析契约测试

**Files:**
- Test: `apps/edge/src/modules/gateway/pairing.rs`（内联 tests)

- [ ] **Step 1: 写测试**

终审指出 pairing ack 解析用 `unwrap_or_default()`,cloud 若缺 `thing_id` 会静默得到空凭据。写测试钉住 co-upgrade 契约（读 pairing.rs 实际解析代码后按其结构写）:

```rust
#[test]
fn pairing_ack_requires_thing_id() {
    let ack = serde_json::json!({
        "credentials": { "thing_id": "gw-1", "password": "p", "username": "u" },
        // 其余必填字段按实际 PairingAck 结构补齐
    });
    // 断言:含 thing_id 的 ack 解析后 credentials.thing_id == "gw-1"
    let parsed = parse_pairing_ack(serde_json::to_vec(&ack).unwrap()).unwrap();
    assert_eq!(parsed.thing_id, "gw-1");
}
```

若现有解析对缺 `thing_id` 静默通过（`unwrap_or_default`),测试加一条缺键情形：**裁决：缺 `thing_id` 应显式报错**(co-upgrade 契约，静默空凭据比报错更难排查）——实现：把该字段的 `unwrap_or_default` 改为返回错误的解析（`ok_or`/map_err)，测试断言 Err。

- [ ] **Step 2: 运行 + Commit**

```bash
cargo test -p tinyiothub-edge pairing
git add -A apps/edge && git commit -m "test(edge): pin pairing ack thing_id contract; fail loudly on missing key"
```

---

### Task 7: 收尾（门禁 + CHANGELOG + TODOS)

**Files:**
- Modify: `CHANGELOG.md`、`TODOS.md`

- [ ] **Step 1: 全量门禁**

```bash
cargo test --workspace 2>&1 | tail -5        # 全绿
cargo clippy --workspace --all-targets -- -D warnings 2>&1 | tail -3
cargo fmt --all -- --check
```

- [ ] **Step 2: CHANGELOG `[Unreleased]` 追加**

```markdown
### Fixed
- MQTT gateway discovery 端到端修复:cloud 路由守卫 off-by-one(6 段 topic 被丢弃)+ edge 发布 topic/payload 对齐 `thing/discover` + `ThingDiscoverMessage`(pre-existing 死链)。
- edge telemetry payload 对齐 cloud `TelemetryMessage` 契约(type/data/timestamp);edge 须与 cloud 同步升级。
- agent tools catalog 工具 id 对齐 MCP 注册名(search_things/get_thing/create_thing/delete_thing);存量 agent tool_overrides 旧名经迁移 `20260831000001` 自动翻转(如调查发现需要)。
- tag stats by_type 恒 0 死桶修复(`device`→`thing`);tombstone 文案补全 `/api/v1` 前缀。
- edge pairing ack 缺 `thing_id` 时显式报错(原静默空凭据)。

### Changed
- apps/cloud 残留 Device* 类型名改 Thing*(纯类型名,wire JSON 键不变)。
```

- [ ] **Step 3: TODOS.md 追加仓级遗留（只记录不修）**

```markdown
- [ ] examples/* 不在 workspace members,未受编译门禁保护;drivers/* 同样非成员(PR-2 遗留)——评估是否纳入 workspace 或 CI 单独构建
- [ ] data/ 真实 dev 库为 legacy 68 迁移链,启动被拒;如需保留数据:旧版本导出 → 新版本重建
- [ ] dead code 记录(按 CLAUDE.md 不删除):crates/db/src/permission.rs 死列查询函数、web thing-cache.updateProperty(不可达且 name/id 语义 gap)、crates/runtime/src/query_service.rs 孤儿文件(不编译)、edge publish_discovery 若 Task 1 未接线则记录两端已对齐待 handler
- [ ] agent tools catalog 与 MCP 注册表的单一事实来源:目前两处手工对齐,考虑派生宏或生成测试长期防漂移
- [ ] ThingEventType::DeviceAlarm 等混合命名:serde wire 兼容的刻意取舍,命名规范文档记一笔
```

- [ ] **Step 4: Commit**

```bash
git add CHANGELOG.md TODOS.md && git commit -m "docs: changelog + todos for thing-rename follow-ups (PR-3)"
```

---

## Self-Review 记录

- **Spec 覆盖**:① MQTT discovery ✅(Task 1)、② telemetry payload ✅(Task 2)、③ tools catalog+agent_tools 迁移 ✅(Task 3)、④ 残留 Device* 类型 ✅(Task 4)、⑤ 小修 ✅(Task 5)、⑥ pairing ack 测试 ✅(Task 6)、⑦ 门禁/CHANGELOG/TODOS ✅(Task 7)。
- **Placeholder 扫描**:Task 1 Step 1 测试的 payload 样例注明"以实际字段为准"(编译驱动);Task 1 Step 3 的 wiring 有明确的两分支决策规则(cloud 有 handler 则接、无则记录);Task 3 Step 1 调查有明确问题清单;无 TBD。
- **类型一致性**:MQTT 契约终态在 Global Constraints 逐字固定,Task 1/2 两端对齐同一形状;catalog 名字源(mcp/tools/thing.rs 行号）在约束与 Task 3 一致。
- **风险备忘**:① edge 改动（Task 1/2/6）与 cloud 同 PR，部署顺序仍是 edge/cloud 同步升级；② Task 3 的迁移是否需要在 Step 1 调查定，若不需要则 commit message 去掉迁移说明；③ get_tag_stats by_type 键变化对外可见但原桶恒 0，前端无消费（终审已确认）。
