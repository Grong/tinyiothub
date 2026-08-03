# Thing-Agent 闭环 E2E 手动验收脚本（T19）

模拟温控设备上报温度超限 → AI 数秒内唤醒 → 查本体 → 调低设定值 → 回读确认
→ chat/Runs API 可见报告 → 同事件再唤醒时记忆显示上次处置、不重复动作。

**红线**：事件必须从真实 MQTT topic 进（`mosquitto_pub` → 平台 MQTT 订阅），
动作必须从真实驱动通道出（`AutonomousInvokeActionTool` → DataServer 命令队列；
无真实驱动注册时平台落 `simulated`，这是产品的真实下发路径而非测试 mock）。
LLM 两档可选（见文末）：档位 A 真实 MiniMax provider（本文步骤）；档位 B
StubLlm（自动化套件，逐断言映射）。

> 观察手段（Runs API / sqlite 查询）不受红线约束——红线只约束事件源与命令通道。

## 0. 前置

- 本地 MQTT broker（平台订阅它）：
  ```bash
  docker run -d --name e2e-mosquitto -p 1883:1883 eclipse-mosquitto:2
  ```
- `app_settings.toml`：
  ```toml
  [mqtt.primary]
  host = "localhost"
  port = 1883
  use_tls = false

  [minimax]
  auth_token = "<真实 token>"   # 档位 A 必填
  model = "MiniMax-M3"
  ```
- 工具：`mosquitto_pub`、`sqlite3`、`jq`、`curl`。

## 1. 启动服务

```bash
cargo run -p tinyiothub-cloud
```

日志确认：`Platform MQTT client connected to localhost:1883`、
`thing-agent loop started`（workspace 创建后）。健康检查：

```bash
curl -s http://localhost:3002/health   # 期望包含 OK
```

## 2. 登录 + 创建工作空间

```bash
BASE=http://localhost:3002/api/v1
TOKEN=$(curl -s -X POST $BASE/auth/login \
  -H 'Content-Type: application/json' \
  -d '{"username":"admin","password":"admin123"}' | jq -r .result.access_token)

WS=$(curl -s -X POST $BASE/workspaces \
  -H "Authorization: Bearer $TOKEN" -H 'Content-Type: application/json' \
  -d '{"name":"e2e-thermo","description":"T19 E2E"}' | jq -r .result.id)
echo "workspace: $WS"
```

## 3. 种子设备 + 物模型（测试数据，非运行时路径）

服务运行中直接写库（sqlite 多连接安全；设备也可在 Web UI 创建）：

```bash
sqlite3 data/tinyiothub.db <<'SQL'
INSERT OR IGNORE INTO devices (id, name, workspace_id, thing_type)
  VALUES ('thermo-01', 'E2E 温控器', 'WS_ID', 'device');
INSERT OR IGNORE INTO thing_actions (id, device_id, name)
  VALUES ('act-set_target_temp', 'thermo-01', 'set_target_temp');
INSERT OR IGNORE INTO thing_properties (id, device_id, name, data_type)
  VALUES ('prop-temp', 'thermo-01', 'temp', 'float');
SQL
```

把 `WS_ID` 替换为 `$WS`。thing-agent loop 在服务启动时为既有 workspace
拉起；新 workspace 由 Orchestrator 回调拉起。完成后重启服务或等回调为该
workspace 拉起 thing-agent loop（种子设备无需重启）。

## 4. 配置自治策略（act + 全量允许）

```bash
curl -s -X PUT $BASE/workspaces/$WS/agent/policy \
  -H "Authorization: Bearer $TOKEN" -H 'Content-Type: application/json' \
  -d '{"mode":"act","allowedActions":["*"],"deniedActions":[],
       "maxActionsPerRun":3,"maxActionsPerHour":30}' | jq .result.mode   # 期望 "act"
```

## 5. 模拟设备上报温度超限（critical → 数秒内唤醒）

```bash
date +%s   # 记录上报时刻 T0
mosquitto_pub -h localhost -t thing/thermo-01/event/temp_high -q 1 -m '{
  "level": "critical",
  "data": {"value": 31.5, "threshold": 26, "unit": "celsius",
           "note": "车间温度超限，需要把目标温度调低到 26"}
}'
```

> 用 `critical` 是为了走 O10 直通：critical 绕过 30s 合并窗口，数秒内唤醒。
> 若发 `warning`，唤醒会在合并窗口（默认 30s）后到达——这是规格行为，不是故障。

**断言点 A — 数秒内唤醒：**

```bash
curl -s "$BASE/workspaces/$WS/agent/runs?limit=5" \
  -H "Authorization: Bearer $TOKEN" | jq '.result.runs[0]'
```

- `triggerType == "thing"`、`triggerContext == "thing:thermo-01:event:temp_high"`
- `createdAt` 与 T0 相差数秒（远小于 30s 合并窗口）

**断言点 B — 查本体 → 调低设定值 → 回读确认：**

- `outcome == "acted"`，`verified == true`（invoke 后 read_property 回读）
- 动作明细（Runs API 不含 actions，直查库）：
  ```bash
  sqlite3 data/tinyiothub.db \
    "SELECT json_extract(report,'$.actions[0].action_name'),
            json_extract(report,'$.actions[0].result.success.status')
     FROM agent_runs WHERE workspace_id='$WS' ORDER BY rowid DESC LIMIT 1"
  # 期望: set_target_temp | executed（有驱动）或 simulated（无驱动，真实下发路径的缺省档）
  ```
- agent 动作已按 actor='agent' 落 events 表（共振防护硬交接）：
  ```bash
  sqlite3 data/tinyiothub.db \
    "SELECT actor, event_subtype FROM events WHERE device_id='thermo-01' AND actor='agent'"
  # 期望: agent | set_target_temp，且该事件没有再唤醒新 Run（runs 总数不变）
  ```

**断言点 C — chat/Runs API 可见报告：**

- `summary` 为自然语言处置报告（含结果与验证结论）
- 无活跃会话时回退为告警：`sqlite3 data/tinyiothub.db "SELECT content FROM events WHERE event_subtype='thing_agent_alert' ORDER BY rowid DESC LIMIT 1"` 含同一 run 摘要
- （可选）先在工作空间开一个 chat 会话再发指令，assistant 消息会直接回推到会话（T13/T14 链路）

## 6. 同事件再唤醒 —— 记忆不重复动作

等第一次 Run 完成后再发一次完全相同的事件：

```bash
mosquitto_pub -h localhost -t thing/thermo-01/event/temp_high -q 1 -m '{
  "level": "critical",
  "data": {"value": 31.5, "threshold": 26, "unit": "celsius",
           "note": "车间温度超限，需要把目标温度调低到 26"}
}'
sleep 15
curl -s "$BASE/workspaces/$WS/agent/runs?limit=2" \
  -H "Authorization: Bearer $TOKEN" | jq '.result.runs[] | {outcome, summary, verified}'
```

预期（真实 LLM 有合理方差，看趋势不看字眼）：

- 第二次唤醒的 prompt 已注入同 dedup_key 的历史处置记录（T10/T12）；
- `summary` 体现出"已知上次已处置"（如先回读温度、判断已恢复正常）；
- 不再盲目重复 `set_target_temp`——常见收敛形态：`outcome == "no_action_needed"`
  或 acted 但动作序列是先查询再决策。

## 7. 恢复

```bash
# 策略关断（kill switch）：后续唤醒应立即停止
curl -s -X PUT $BASE/workspaces/$WS/agent/policy \
  -H "Authorization: Bearer $TOKEN" -H 'Content-Type: application/json' \
  -d '{"mode":"off"}' | jq .result.mode   # "off"

# 验证关断：再发一次 critical，观察 30s，runs 总数不变
sqlite3 data/tinyiothub.db "SELECT COUNT(*) FROM agent_runs WHERE workspace_id='$WS'"
mosquitto_pub -h localhost -t thing/thermo-01/event/temp_high -q 1 -m '{"level":"critical","data":{"value":35}}'
sleep 30
sqlite3 data/tinyiothub.db "SELECT COUNT(*) FROM agent_runs WHERE workspace_id='$WS'"   # 与上次相同

# 清理
docker rm -f e2e-mosquitto
sqlite3 data/tinyiothub.db "DELETE FROM devices WHERE id='thermo-01'"
```

## 档位 B：StubLlm（无 LLM key 时的等效验收）

红线链路（MQTT 路径同款 `route_thing_event` → 触发器 → 调度器 → runner →
策略门 → 驱动下发 → 落库 → 回推）由自动化套件以脚本化 provider 全覆盖，
与本文断言点一一对应：

```bash
cargo test -p tinyiothub-cloud --lib thing_agent_loop
```

| 本文断言点 | 对应测试 |
|---|---|
| A 数秒内唤醒（critical 直通） | `critical_event_bypasses_30s_merge_window_end_to_end` |
| B 查本体→调低→回读 verified | `warning_event_runs_full_loop_and_persists_verified_report`（T15） |
| C Runs API/告警可见报告 | 同上 + `policy_denial_streak_triggers_relax_hint_with_real_repo` |
| 6 记忆不重复（合并/去重） | `five_events_in_30s_merge_into_one_wake`（T15）+ `duplicate_directive_via_dispatch_tool_yields_single_run` |
| 7 mode=off 关断 | `mode_off_suppresses_event_and_timer_wakes_end_to_end` |
| 队列上限/挂起/注入 | `queue_full_51st_directive_rejected_and_user_informed` / `hung_llm_run_forced_closed_as_budget_exceeded` / `injected_event_payload_cannot_bypass_denylist` |
