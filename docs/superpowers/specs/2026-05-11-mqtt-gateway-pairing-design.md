# MQTT 网关配对注册设计

## 概述

边缘网关通过 MQTT 协议零配置注册到 TinyIoTHub 平台。网关开机显示 6 位配对码，用户在平台上输入，即可完成设备绑定。

**核心目标：**
- 用户无感注册，不需要编辑配置文件、不需要 SSH
- 设备列表扁平展示，网关和子设备同级
- 基于外部 MQTT Broker（mqtt.tinyiothub.com）

---

## 整体架构

```
┌─────────────────────────────────────────┐
│              TinyIoTHub 平台              │
│                                          │
│  ┌──────────┐  ┌──────────────────────┐  │
│  │ 前端 UI   │  │ cloud/src/modules/   │  │
│  │ 配对输入  │  │   gateway/            │  │
│  │ 设备列表  │  │   ├── types.rs        │  │
│  └──────────┘  │   ├── service.rs      │  │
│                 │   ├── handler/        │  │
│  ┌──────────┐  │   └── pairing.rs      │  │
│  │ MQTT     │  │       (配对码缓存)     │  │
│  │ Platform │◄─┤                      │  │
│  │ Client   │  └──────────────────────┘  │
│  └────┬─────┘                            │
└───────┼──────────────────────────────────┘
        │ MQTT
  ┌─────┴──────┐
  │ mqtt.       │
  │ tinyiothub  │
  │ .com        │
  └─────┬──────┘
        │ MQTT
┌───────┼──────────────────────────────────┐
│ ┌─────┴─────┐                            │
│ │ MQTT      │  edge/ (边缘网关)           │
│ │ Gateway   │  ├── mqtt_client.rs         │
│ │ Client    │  ├── pairing.rs             │
│ └───────────┘  ├── device_discovery.rs    │
│                 └── config.rs             │
│  边缘网关 (有屏幕/网页管理界面)            │
└──────────────────────────────────────────┘
```

**关键设计决策：**
- 平台和网关都是 MQTT client，连接同一个外部 broker
- Broker 允许匿名连接 `tinyiothub/pairing/#`，认证连接其他 topic
- 配对成功后平台下发正式凭据，网关断开匿名重连

---

## 用户流程

```
网关插电开机
    ↓
网关屏幕显示 6 位配对码：「482 916」
    ↓
用户登录平台，点击「添加设备」→ 选择「网关设备」
    ↓
输入配对码 482 916
    ↓
配对成功 → 网关出现在设备列表中，状态在线
```

**用户操作量：** 选择类型 + 输入 6 位数字，< 10 秒完成。

---

## 配对协议

### 配对宣告（网关 → 平台）

- Topic: `tinyiothub/pairing/announce`
- QoS: 1
- 重发间隔: 30s，直到成功或配对码过期（5 分钟）

```json
{
  "type": "pairing_announce",
  "code": "482916",
  "fingerprint": "aa:bb:cc:dd:ee:ff",
  "hostname": "factory-gw-01",
  "os": "Linux armv7l",
  "ip": "192.168.1.100",
  "hw_model": "Raspberry Pi 5"
}
```

### 配对响应（平台 → 网关）

- Topic: `tinyiothub/pairing/{code}/response`
- QoS: 1

```json
{
  "type": "pairing_ack",
  "success": true,
  "device_id": "dev_xyz",
  "workspace_id": "ws_abc",
  "credentials": {
    "client_id": "dev_xyz",
    "username": "dev_xyz",
    "password": "dt_abc123..."
  },
  "topics": {
    "status": "tinyiothub/ws_abc/gateway/dev_xyz/status",
    "telemetry": "tinyiothub/ws_abc/gateway/dev_xyz/telemetry",
    "event": "tinyiothub/ws_abc/gateway/dev_xyz/event",
    "command": "tinyiothub/ws_abc/gateway/dev_xyz/command",
    "config": "tinyiothub/ws_abc/gateway/dev_xyz/config",
    "device_discover": "tinyiothub/ws_abc/gateway/dev_xyz/device/discover",
    "device_telemetry": "tinyiothub/ws_abc/gateway/dev_xyz/device/+/telemetry"
  },
  "keepalive": 60
}
```

---

## MQTT Topic 结构

```
tinyiothub/
├── pairing/
│   ├── announce                       # 网关→平台：宣告配对码
│   └── {code}/response                # 平台→网关：凭证下发
│
└── {ws_id}/
    └── gateway/{gw_id}/
        ├── status                     # 网关→平台：状态/心跳
        ├── telemetry                  # 网关→平台：遥测数据
        ├── event                      # 网关→平台：告警
        ├── command                    # 平台→网关：指令
        ├── config                     # 平台→网关：配置下发
        └── device/
            ├── discover               # 网关→平台：子设备发现
            └── {sub_id}/
                ├── telemetry          # 子设备遥测（网关代为上报）
                ├── event              # 子设备告警
                └── command            # 平台→网关→子设备：指令
```

### QoS 策略

| Topic | QoS | 说明 |
|-------|-----|------|
| pairing/announce | 1 | 保证到达 |
| pairing/{code}/response | 1 | 保证到达 |
| status | 0 | 周期性，丢一次无影响 |
| telemetry | 0 | 高频数据 |
| event | 1 | 告警不能丢 |
| command | 1 | 指令必须送达 |
| config | 1 | 配置必须送达 |

---

## 配对码规则 & 安全

### 配对码生成（网关侧）

- 6 位纯数字（`000000` ~ `999999`），显示格式 `XXX XXX`
- 随机生成，非递增
- 有效期 5 分钟，到期自动刷新
- 刷新后旧码立即失效

### 安全约束（平台侧）

| 规则 | 说明 |
|------|------|
| 尝试次数限制 | 同一用户同一配对码最多 5 次，超限锁定 1 分钟 |
| IP 限流 | 同一 IP 每分钟最多 3 次配对校验 |
| 内存存储 | code → fingerprint 映射仅存内存，不落盘 |
| 一次性 | 配对成功后立即失效，不可重复使用 |
| 无 workspace 关联 | code 不预绑定用户/工作空间，谁输入正确谁配对 |
| 匿名 topic 限制 | MQTT broker 只允许匿名连接访问 `tinyiothub/pairing/#` |

### 平台内存缓存

```rust
struct PairingCache {
    // key = pairing_code
    entries: HashMap<String, PairingEntry>,
    max_entries: usize,  // 硬上限，默认 10000
}

struct PairingEntry {
    fingerprint: String,
    hostname: String,
    os: String,
    ip: String,
    hw_model: String,
    created_at: Instant,      // 5 分钟过期自动清理
    attempts: HashMap<UserId, u32>,  // 按用户尝试次数
}
```

并发保护：`PairingCache` 用 `Arc<RwLock<HashMap<...>>>` 包裹，与现有 `rate_limit.rs:47` 一致。

宣告全局限流：在 announce 消息处理入口用 token bucket 限流（每秒 20 个宣告、burst 50），超限直接丢弃不写 cache，防止恶意/故障网关刷码填满 cache。

### 缓存满处理

当 `entries.len() >= max_entries` 时，拒绝新宣告：日志记录 WARN，MQTT 不做任何响应。网关收不到 pairing_ack 会持续重发，过期淘汰后自然恢复。`max_entries=10000` 远超正常部署规模（同时 100 个未配对网关 = 100 个条目）。

### 配对校验事务性（关键）

配对成功时先创建 Device（写 DB），**然后**发布 MQTT 响应。如果 MQTT publish 失败：

1. Device 已写入 DB → 回滚：删除刚创建的 Device
2. 回滚成功后，从 cache 移除该 code
3. 返回 HTTP 500 + "配对暂时失败，请稍后重试，无需重新输入配对码"

确保不会出现"用户看到成功、网关收不到凭据"的半状态。

```
配对流程事务边界：

  code 存在且未过期?
      ↓ Y
  max_attempts 未超?
      ↓ Y
  创建 Device (DB write)        ← 事务边界开始
      ↓
  发布配对响应 (MQTT publish)   ← 如果失败，回滚 Device
      ↓
  code 从 cache 移除            ← 事务边界结束
      ↓
  返回 HTTP 200 + device_id
```

---

## 子设备模型

### 设计原则

- 设备列表扁平一层，不嵌套
- 通过 `parent_id` 和 `linked_gateway` 后台关联
- 子设备数据和指令均经由网关 MQTT topic 中转

### 设备列表示例（前端平铺）

```
┌─────────────────────────────────────────────────────────┐
│ 设备列表                                                  │
│                                                          │
│  🏠 工厂网关           在线    MQTT                       │
│  🌡️ 温度传感器-1       在线    Modbus · via 工厂网关       │
│  🔧 电磁阀-2          在线    Modbus · via 工厂网关       │
│  ⚡ 电流表-3          离线    Modbus · via 工厂网关       │
│  📡 仓库网关           在线    MQTT                       │
│  💨 风机-1            在线    Modbus · via 仓库网关       │
└─────────────────────────────────────────────────────────┘
```

### 子设备发现协议（网关 → 平台）

- Topic: `tinyiothub/{ws_id}/gateway/{gw_id}/device/discover`
- QoS: 1

```json
{
  "type": "device_discover",
  "devices": [
    {
      "name": "温度传感器-1",
      "device_type": "sensor",
      "protocol_type": "modbus",
      "address": "192.168.1.10:502",
      "driver_name": "modbus-tcp",
      "driver_options": "{\"register\":40001,\"function\":3}"
    },
    {
      "name": "电磁阀-2",
      "device_type": "actuator",
      "protocol_type": "modbus",
      "address": "192.168.1.10:502",
      "driver_name": "modbus-tcp",
      "driver_options": "{\"register\":40002,\"function\":6}"
    }
  ]
}
```

### 子设备上线判断

| 场景 | 处理 |
|------|------|
| 网关在线 + 子设备数据正常上报 | 子设备 = 在线 |
| 网关在线 + 子设备超时未上报 | 子设备 = 离线（超时阈值可配置，默认 2 倍心跳间隔） |
| 网关离线 | 所有子设备自动标记离线 |

---

## 数据上报 & 指令下发

### 遥测上报（网关自身）

- Topic: `tinyiothub/{ws_id}/gateway/{gw_id}/telemetry`
- QoS: 0
- 频率: 可配置（默认 30s）

```json
{
  "type": "telemetry",
  "data": {
    "cpu": 23.5,
    "memory": 45.2,
    "disk": 32.1,
    "network_rx": 1024000,
    "network_tx": 512000
  },
  "timestamp": 1715432000
}
```

### 遥测上报（子设备）

- Topic: `tinyiothub/{ws_id}/gateway/{gw_id}/device/{sub_id}/telemetry`
- QoS: 0

```json
{
  "type": "device_telemetry",
  "device_id": "dev_sub_001",
  "data": {
    "temperature": 25.3,
    "humidity": 68.5
  },
  "timestamp": 1715432000
}
```

### 指令下发（平台 → 网关 → 子设备）

- Topic: `tinyiothub/{ws_id}/gateway/{gw_id}/device/{sub_id}/command`
- QoS: 1

```json
{
  "type": "command",
  "command_id": "cmd_001",
  "device_id": "dev_sub_001",
  "action": "set_value",
  "params": {
    "register": 40002,
    "value": 1
  },
  "timestamp": 1715432000
}
```

---

## 错误处理

### API 错误响应

| 场景 | HTTP Code | msg |
|------|-----------|-----|
| 配对码未找到 | 404 | "未发现设备，请确认配对码是否正确" |
| 配对码已过期（5 分钟） | 410 | "配对码已过期，请查看网关屏幕上的新配对码" |
| 尝试次数过多 | 429 | "尝试次数过多，请 1 分钟后重试" |
| IP 限流 | 429 | "请求过频，请稍后重试" |
| 缓存满 | 503 | "服务繁忙，请稍后重试" |
| 设备创建/回滚失败 | 500 | "配对暂时失败，请稍后重试" |

### 网关侧错误处理

| 场景 | 网关行为 |
|------|----------|
| 宣告发布失败（broker 不可达） | 指数退避重连（1s → 2s → 4s → 8s，最大 30s） |
| 配对响应超时（5 分钟内无响应） | 刷新配对码，重新宣告 |
| 正式凭据连接失败 | 重试 3 次（间隔 5s），失败后回退到匿名配对模式 |
| 子设备指令超时 | 返回错误事件到 event topic，不阻塞后续指令 |

---

---

## 部署配置

### MQTT Broker ACL (Mosquitto)

配对阶段网关匿名连接，仅能访问配对 topic。需修改 mosquitto.conf：

```conf
# 允许匿名连接（受 ACL 限制）
allow_anonymous true
password_file /mosquitto/config/passwd
acl_file /mosquitto/config/acl

# ...其他现有配置不变
```

新建 `deploy/docker/mosquitto/config/acl`：

```
# 匿名客户端：只能访问配对 topic
user anonymous
topic readwrite tinyiothub/pairing/#

# 平台客户端：完整访问
user admin
topic readwrite tinyiothub/#
```

**topic 访问矩阵：**

| 角色 | 用户 | publish | subscribe |
|------|------|---------|-----------|
| 配对中的网关 | anonymous | `pairing/announce` | `pairing/{code}/response` |
| 平台服务 | admin (已配 MQTT_PASSWORD) | `pairing/{code}/response`, `{ws_id}/gateway/{gw_id}/command`, `{ws_id}/gateway/{gw_id}/config` | `pairing/announce`, `{ws_id}/gateway/+/status`, `{ws_id}/gateway/+/telemetry`, `{ws_id}/gateway/+/event`, `{ws_id}/gateway/+/device/#` |
| 已注册网关 | `dev_xyz` (配对后下发) | 自身 workspace 下 topic | 自身 command/config topic |


## 数据库变更

### device 表新增字段

```sql
ALTER TABLE devices ADD COLUMN linked_gateway TEXT;  -- 子设备关联的网关 device_id
ALTER TABLE devices ADD COLUMN fingerprint TEXT;      -- 网关硬件指纹（MAC 等）
```

- `protocol_type` = `"mqtt"` 表示网关设备（自身直连云平台）
- `linked_gateway` 非空表示该设备通过某网关上云，值为网关的 device_id
- `parent_id` 保留原有语义不变（层级关系，与 linked_gateway 独立）
- 子设备的 linked_gateway = 网关 device_id，数据上报路由由此字段决定

### 无需新建表

复用现有 `devices` 表，不引入 `gateway_tokens` 或独立网关表。

---

## 平台端变更

| 组件 | 位置 | 说明 |
|------|------|------|
| 配对码内存缓存 | `cloud/src/modules/gateway/pairing.rs` | HashMap + 定时清理 |
| Gateway 模块 (types/service/handler) | `cloud/src/modules/gateway/` | 配对校验 API + 设备发现处理 |
| 平台 MQTT Client | `cloud/src/shared/mqtt_client.rs` | 订阅配对/遥测/事件 topic |
| Device 模型扩展 | `crates/tinyiothub-core/src/models/device.rs` | 新增 linked_gateway、fingerprint |
| 前端添加设备页 | `web/src/ui/` | 网关配对码输入界面 |
| 前端设备列表 | `web/src/ui/` | 显示"via 网关名称"标签 |

### 可观测性

- 配对事件结构化日志（含 code、fingerprint、成功/失败原因、latency）
- 配对成功率 metric（简单计数器：`pairing_attempts_total` / `pairing_successes_total`）
- MQTT 连接状态 metric（平台 MQTT client 是否连上 broker）

### API 变更

| 方法 | 路径 | 说明 |
|------|------|------|
| POST | `/api/v1/gateway/pair` | 输入配对码，返回配对结果 |
| GET | `/api/v1/devices?linked_gateway={gw_id}` | 查询某网关下的子设备 |
| POST | `/api/v1/gateway/command` | 向网关/子设备下发指令 |

---

## 边缘网关变更

| 组件 | 说明 |
|------|------|
| 配对码生成 | 6 位随机数字，5 分钟刷新，屏幕显示 |
| 匿名 MQTT 连接 | 连接 broker，发布配对宣告，等待响应 |
| 认证重连 | 收到凭据后断开匿名连接，用正式凭据重连 |
| 凭据持久化 | 配对成功后将 `device_id` + `password` 写入本地 JSON 文件，断电重启后直接读取重连，无需重新配对 |
| 心跳上报 | 周期性发布 status |
| 子设备发现 | 扫描本地 Modbus/ONVIF 等设备，上报 discover |
| 数据代理 | 采集子设备数据，代为上报到子设备 topic |
| 指令转发 | 监听子设备 command topic，转发到本地协议 |

---

## 复用已有代码

- **Device CRUD** — 复用 `cloud/src/modules/device/` 和 `crates/tinyiothub-storage/src/sqlite/device.rs`
- **设备模型** — 复用 `crates/tinyiothub-core/src/models/device.rs`，新增 2 个字段
- **ApiResponseBuilder** — 复用 `tinyiothub-web::response::ApiResponseBuilder`
- **前端 API Client** — 复用 `web/src/api/client.ts`
- **MQTT 依赖** — 复用 `rumqttc`（已在项目中）

---

## NOT in scope

- **网关 OTA 固件升级** — v0.2+ 功能，当前 spec 聚焦注册和数据通道
- **网关群组/批量管理** — 单网关注册流程，不涉及多网关同时操作
- **MQTT 消息持久化/离线消息队列** — 网关离线期间的消息缓存待后续版本
- **MQTT over TLS/SSL** — broker 已支持 TLS（nginx 反向代理 8883），但网关侧 TLS 配置留待部署文档
- **配对码格式扩展（二维码/蓝牙/NFC）** — 当前仅 6 位数字屏幕显示
- **子设备移除/注销** — 发现协议只处理新增，子设备移除逻辑待 v0.2

## What already exists

| 已有组件 | 位置 | 复用方式 |
|----------|------|----------|
| Device 模型 (protocol_type, parent_id) | `crates/tinyiothub-core/src/models/device.rs` | 新增 linked_gateway, fingerprint 字段 |
| Device CRUD + Repository | `crates/tinyiothub-storage/`, `cloud/src/modules/device/` | 网关注册后创建 Device，复用手法 |
| MQTT 依赖 (rumqttc) | `Cargo.toml` (workspace dep) | 平台 + 网关均使用 |
| ApiResponseBuilder | `tinyiothub-web::response` | 所有新 API 使用 |
| Arc<RwLock<HashMap>> 模式 | `rate_limit.rs:47` | PairingCache 并发保护 |
| Mosquitto broker | `deploy/docker/docker-compose.yml` | 新增 ACL 配置 |
| 前端 API Client | `web/src/api/client.ts` | 新增 gateway/pair 调用 |

## Dream state delta

```
v0.1 (本 spec)              v0.2                    v0.3
配对码注册网关              扫码/蓝牙配对           自发现（网关局域网广播）
子设备自动发现              子设备模板匹配            子设备即插即用
基础遥测/指令               规则引擎告警              边缘侧规则执行
MQTT 明文                   MQTT over TLS           端到端加密
本地凭据持久化              远程凭据轮换              OTA 固件升级
```

本 spec 是正确的基础。不贪多，每一步都有清晰的退出条件。
