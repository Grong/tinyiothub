# Edge Gateway v0.2 — 完整架构设计

> 设计日期: 2026-05-13
> 状态: Reviewed（/plan-ceo-review, SCOPE EXPANSION, 6/6 proposals accepted）
> 前置: MQTT Gateway Pairing v0.1（已完成）
> CEO Plan: ~/.gstack/projects/Grong-tinyiothub/ceo-plans/2026-05-13-edge-gateway-full.md

## 概述

Target: 将 edge gateway 从当前骨架（配对 + 心跳，约 175 行）升级为完整的边缘网关运行时。

Core insight: **Edge = Cloud 精简版 + MQTT 通信 + 边缘特性**。Edge 直接复用 `tinyiothub-core`（traits + 模型 + rule engine）、`tinyiothub-storage`（SQLite repos）、`tinyiothub-runtime`（协议驱动）、`tinyiothub-error`（错误类型）四个已有 crate，只在上层写一个精简的服务编排层。

运行环境: 嵌入式 Linux 裸进程，512MB-1GB RAM，4-8GB 存储。

## 架构概览

```
┌─────────────────────────────────────────────────┐
│  Cloud (SaaS)                                    │
│  HTTP/Axum → modules/ → services → repositories  │
│  Communication: HTTP (in) + MQTT (out to edge)   │
└────────────────────┬────────────────────────────┘
                     │ MQTT
                     │ tinyiothub/{ws}/gateway/{gw_id}/...
                     │
┌────────────────────┴────────────────────────────┐
│  Edge (Embedded Linux)                           │
│  MQTT → services → repositories (local SQLite)   │
│                                                   │
│  ┌──────────┐ ┌──────────┐ ┌──────────────┐      │
│  │ Gateway  │ │ Command  │ │  Telemetry   │      │
│  │ Service  │ │ Service  │ │   Service    │      │
│  └──────────┘ └──────────┘ └──────────────┘      │
│  ┌──────────┐ ┌──────────┐ ┌──────────────┐      │
│  │ Config   │ │  Device  │ │   Health     │      │
│  │ Service  │ │ Service  │ │   Service    │      │
│  └──────────┘ └──────────┘ └──────────────┘      │
│  ┌──────────┐ ┌──────────┐ ┌──────────────┐      │
│  │  Driver  │ │ Offline  │ │ Intelligence │      │
│  │ Service  │ │  Buffer  │ │   Service    │      │
│  └──────────┘ └──────────┘ └──────────────┘      │
│  ┌──────────────────────────────────────┐        │
│  │         HTTP Server (optional)       │        │
│  │         127.0.0.1, 12 endpoints      │        │
│  └──────────────────────────────────────┘        │
│                                                   │
│  Reused Crates:                                   │
│  tinyiothub-core (traits + models + rule engine)  │
│  tinyiothub-storage (SQLite repos)                │
│  tinyiothub-runtime (protocol drivers)            │
│  tinyiothub-error (error types)                   │
└─────────────────────────────────────────────────┘
```

## 模块映射：Cloud → Edge（合并后 9 模块）

| Cloud 模块 | Edge 模块 | 说明 |
|-----------|----------|------|
| `device` | `device` 精简版 | 本地设备 CRUD + 属性读写，复用 DeviceRepository |
| `drivers` | `driver` 直接用+扩展 | 复用 runtime 驱动 + 动态 .so 加载 |
| `gateway` | `gateway` 已有+增强 | 配对 + MQTT 通信 + 消息路由 |
| `alarm` + `self_healing` | `intelligence` 合并 | 本地告警评估 + 自愈探针 |
| `event` | `telemetry` 精简版 | 遥测采集 + 变换 + 转发 |
| `heartbeat` + `monitoring` | `health` 合并 | 心跳上报 + 资源监控 |
| `template` | `config_mgmt` 精简版 | 从 cloud 同步，本地 YAML 存储 |
| `cron` | 整合到各 service | 定时器由事件循环管理 |
| — | `command` 新增 | 接收 cloud 指令 → 路由到驱动 |
| — | `offline` 新增 | SQLite 离线缓冲 + 分级淘汰 |
| `auth/user/role/permission` | 不需要 | — |
| `tenant/workspace` | 不需要 | — |
| `agent/chat/mcp` | 不需要 | — |
| `marketplace` | 不需要 | — |
| `notification` | 不需要 | — |
| `plugin` | 不需要 | — |
| `tag` | 不需要 | — |

## 目录结构

```
edge/src/
├── main.rs                  # 入口 + 事件循环
├── config.rs                # EdgeConfig + GatewayCredentials (已有)
├── app_state.rs             # AppState 组装
├── modules/
│   ├── gateway/             # MQTT 通信 + 配对
│   │   ├── types.rs
│   │   ├── service.rs       # MQTT client, publish/subscribe, 消息路由
│   │   └── pairing.rs       # 配对码 + announce (已有)
│   ├── device/              # 本地设备管理
│   │   ├── types.rs
│   │   └── service.rs       # CRUD、属性/指令代理、get_driver_for_device
│   ├── driver/              # 驱动管理
│   │   ├── types.rs
│   │   └── service.rs       # 驱动生命周期、扫描发现、动态 .so 加载
│   ├── telemetry/           # 遥测采集
│   │   ├── types.rs
│   │   └── service.rs       # 定时采集、变换、转发
│   ├── command/             # 指令执行
│   │   ├── types.rs
│   │   └── service.rs       # 接收 cloud 指令 → 路由到驱动
│   ├── config_mgmt/         # 配置管理
│   │   ├── types.rs
│   │   └── service.rs       # 本地 YAML + cloud 下发覆盖 + 原子写入
│   ├── intelligence/        # 告警 + 自愈（合并）
│   │   ├── types.rs
│   │   └── service.rs       # 规则评估、告警生成、探针调度、自动恢复
│   ├── offline/             # 离线缓冲
│   │   ├── types.rs
│   │   └── service.rs       # SQLite 缓冲、分级淘汰、连接恢复后回放
│   ├── health/              # 心跳 + 监控（合并）
│   │   ├── types.rs
│   │   └── service.rs       # 心跳上报、资源监控、HealthReport
│   └── http/                # 本地 HTTP API（可选，EDGE_LOCAL_API=1 启用）
│       ├── types.rs
│       ├── service.rs       # Axum server, 12 endpoints
│       └── auth.rs          # API key 鉴权
└── shared/
    ├── error.rs             # Edge 错误类型（含 thiserror 派生）
    └── storage.rs           # SQLite 初始化 + migration
```

## MQTT Topic 设计

```
Edge → Cloud（上报）:
  tinyiothub/{ws}/gateway/{gw}/status           # 心跳 + HealthReport
  tinyiothub/{ws}/gateway/{gw}/telemetry         # 设备遥测数据
  tinyiothub/{ws}/gateway/{gw}/device/discover   # 设备发现结果
  tinyiothub/{ws}/gateway/{gw}/alarm             # 告警事件
  tinyiothub/{ws}/gateway/{gw}/log               # 日志上报（可选）
  tinyiothub/{ws}/gateway/{gw}/config/ack         # 配置确认

Cloud → Edge（下发）:
  tinyiothub/{ws}/gateway/{gw}/command           # 指令下发
  tinyiothub/{ws}/gateway/{gw}/config             # 完整配置下发
  tinyiothub/{ws}/gateway/{gw}/config/device      # 单设备配置变更（新增）
  tinyiothub/{ws}/gateway/{gw}/driver/install     # 驱动 .so 分块传输（新增）

Pairing（认证前）:
  tinyiothub/pairing/announce                     # Edge 广播配对码
  tinyiothub/pairing/ack                          # Cloud 返回凭证
```

### config/device topic payload（Scope #3）

```json
{
  "device_id": "dev_xxx",
  "action": "update_property|delete|enable|disable",
  "property": "temperature_threshold",
  "value": 75.0
}
```

## AppState

```rust
pub struct AppState {
    pub config: EdgeConfig,
    pub credentials: GatewayCredentials,
    pub db: Arc<Database>,
    pub device_repo: Arc<dyn DeviceRepository>,
    pub device_service: Arc<DeviceService>,
    pub driver_service: Arc<DriverService>,
    pub telemetry_service: Arc<TelemetryService>,
    pub command_service: Arc<CommandService>,
    pub config_service: Arc<ConfigService>,
    pub intelligence_service: Arc<IntelligenceService>,
    pub gateway_service: Arc<GatewayService>,
    pub offline_buffer: Arc<OfflineBuffer>,
    pub health_service: Arc<HealthService>,
    pub http_server: Option<Arc<HttpServer>>,
}
```

**初始化顺序（避免循环依赖）：**
1. 无依赖：`Database`, `EdgeConfig`, `GatewayCredentials`, `OfflineBuffer`
2. 依赖 step1：`DeviceService`, `GatewayService`, `DriverService`
3. 依赖 step2：`TelemetryService`, `CommandService`, `ConfigService`, `HealthService`, `IntelligenceService`
4. 最后：`HttpServer`（optional，依赖所有 service）

## 事件循环（main.rs）

```rust
async fn run_authenticated(config: EdgeConfig, creds: GatewayCredentials) -> ! {
    let state = AppState::new(config, creds).await?;

    // 1. 从 cloud 拉配置，失败则用本地默认值（Scope #1 自治模式）
    if let Err(e) = state.config_service.sync_from_cloud().await {
        tracing::warn!(?e, "Cloud unreachable, starting in autonomous mode");
        state.config_service.load_defaults();
    }

    // 2. 首次设备扫描
    state.driver_service.scan_all().await?;

    // 3. 回放离线缓冲
    if let Ok(count) = state.offline_buffer.flush(&state.gateway_service).await {
        tracing::info!(count, "Flushed offline buffer");
    }

    // 4. 定时器
    let mut telemetry_tick = tokio::time::interval(state.config.telemetry_interval);
    let mut heartbeat_tick = tokio::time::interval(state.config.heartbeat_interval);
    let mut intelligence_tick = tokio::time::interval(state.config.intelligence_interval);

    loop {
        // 连接断开 → 自动重连 + 离线缓冲
        if !state.gateway_service.mqtt.is_alive() {
            state.offline_buffer.activate();
            state.gateway_service.mqtt.reconnect().await;
            state.offline_buffer.flush(&state.gateway_service).await.ok();
            state.config_service.sync_from_cloud().await.ok();
        }

        tokio::select! {
            _ = telemetry_tick.tick() => {
                if let Err(e) = state.telemetry_service.collect_and_forward().await {
                    tracing::warn!(?e, "Telemetry collect failed");
                }
            }
            _ = heartbeat_tick.tick() => {
                state.health_service.beat_and_report().await.ok();
            }
            _ = intelligence_tick.tick() => {
                state.intelligence_service.evaluate_and_probe().await.ok();
            }
            msg = state.gateway_service.recv() => {
                state.route_message(msg).await;
            }
        }
    }
}
```

## 模块设计（摘要，完整设计见下文）

### Gateway Service
- MQTT 连接管理（指数退避重连，最大 5min）
- 下行消息解析为 `GatewayMessage` 枚举（Command/Config/ConfigDevice/DriverInstall）
- 通过 bounded channel (1000) 发给事件循环
- 提供 `publish_*` 便捷方法

### Device Service
- 复用 `DeviceRepository` trait + SQLite 实现
- 提供 `get_driver_for_device()`——CommandService 不直接依赖 DriverRegistry，避免 DRY 重复
- `sync_from_cloud()` 批量创建/更新设备

### Driver Service
- 复用 `tinyiothub-runtime` 的 Modbus/ONVIF/SNMP/MQTT 驱动
- 新增动态 `.so` 加载（libloading + SHA256 签名验证）
- `scan_all()` 并行扫描，Semaphore 限制并发数
- 扫描去重锁（`AtomicBool scanning`）

### Telemetry Service
- 并行读取设备属性（`try_join_all`）
- 变换规则（JS 表达式，失败回退到原始值）
- 上报失败 → OfflineBuffer

### Command Service
- 通过 DeviceService 获取驱动（不直接依赖 DriverRegistry）
- 执行结果通过 telemetry topic 回传 cloud

### Config Mgmt Service
- 本地 YAML baseline + cloud 下发覆盖
- 原子写入（写 tmp + rename）防止崩溃时破坏配置
- `config/device` topic 接收单设备变更

### Offline Buffer
- 分级淘汰策略（Scope #4）：
  - Telemetry/Log 超过 100000 条 → FIFO 覆盖最旧
  - Alarm 告警永久保留不覆盖
  - 磁盘剩余 < 10% → 拒绝非 alarm 写入
  - 磁盘剩余 < reserved 5MB → 拒绝所有写入 + critical alarm
- 回放每次 500 条，成功即删，失败留到下一批

### Intelligence Service（alarm + self_healing 合并）
- 复用 `tinyiothub-core` rule engine 评估告警规则
- 自愈探针：`device_reachable`, `device_unresponsive`, `memory_high`
- 不健康时自动恢复（驱动重连/重启指令/清理缓存）
- `catch_unwind` 防止探针 panic 影响事件循环

### Health Service（heartbeat + monitor 合并）
- 定时上报在线状态 + timestamp + uptime
- 附带完整 HealthReport（CPU/Mem/Disk + driver 状态 + buffer 积压量）

### HTTP Server（Scope #6）
- 仅 127.0.0.1 监听，默认关闭（`EDGE_LOCAL_API=1`）
- 鉴权：`EDGE_LOCAL_API_KEY` 环境变量（Bearer token）
- 12 个端点（见下方）
- 响应格式对齐 `ApiResponseBuilder`（和 cloud API 一致）

### HTTP API 端点

```
GET    /api/v1/health                     # HealthReport JSON
GET    /api/v1/devices                    # 设备列表 + 在线状态
GET    /api/v1/devices/:id                # 单个设备详情
GET    /api/v1/devices/:id/properties     # 设备属性（实时读取）
POST   /api/v1/devices/:id/properties     # 写入设备属性
POST   /api/v1/devices/:id/command        # 执行设备指令
GET    /api/v1/drivers                    # 已加载驱动列表 + 状态
POST   /api/v1/drivers/scan               # 触发设备扫描
GET    /api/v1/alarms                     # 当前活跃告警
GET    /api/v1/config                     # 当前配置
PUT    /api/v1/config                     # 更新本地配置
GET    /api/v1/offline-buffer             # 离线缓冲状态 + 积压量
```

## SQLite 表设计

已有表（`tinyiothub-storage` 提供，Edge 直接复用）:
- `devices` — DeviceRepository 管理的设备表
- `device_properties` — 设备属性
- `device_commands` — 指令记录

Edge 新增 2 个表:

```sql
-- 离线缓冲表（含分级淘汰字段）
CREATE TABLE IF NOT EXISTS offline_buffer (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    msg_type TEXT NOT NULL,       -- "telemetry" | "alarm" | "log"
    topic TEXT,
    payload BLOB NOT NULL,
    created_at INTEGER NOT NULL,  -- Unix timestamp ms
    retry_count INTEGER DEFAULT 0,
    priority INTEGER DEFAULT 0    -- 0=telemetry, 1=alarm (永久保留)
);

-- 配置版本跟踪
CREATE TABLE IF NOT EXISTS config_meta (
    key TEXT PRIMARY KEY,
    cloud_version TEXT,
    local_version TEXT,
    updated_at INTEGER NOT NULL
);
```

告警规则存储在 config.yaml，本地告警事件暂存内存（连接恢复即上报）。如离线期间 edge 重启导致告警丢失——可接受（告警已通过 offline_buffer 持久化为 alarm 消息）。

## 错误与救援映射（CRITICAL GAPS 已修复）

### CRITICAL GAP #1: AuthError → 清除凭证回退配对
MQTT 认证失败（凭证被 cloud revoke）时，清除 `credentials.json`，自动回退到 pairing 模式。
```rust
// GatewayService::connect
Err(AuthError) => {
    tracing::error!("MQTT auth rejected, clearing credentials and entering pairing mode");
    std::fs::remove_file(&config.credentials_file)?;
    return run_pairing(config).await;
}
```

### CRITICAL GAP #2: LibraryLoadError / SymbolNotFound → 上报 cloud
驱动动态加载失败后通过 status topic 上报 cloud，cloud 在 marketplace UI 显示安装状态。
```rust
Err(LibraryLoadError | SymbolNotFound) => {
    health_service.report_driver_error(driver_name, &error).await;
}
```

### CRITICAL GAP #3: StorageError ×3 → Critical Alarm + SQLite repair
连续 3 次 StorageError 触发 critical alarm，自愈模块尝试 `PRAGMA integrity_check`。
```rust
if consecutive_storage_errors >= 3 {
    intelligence_service.raise_system_alarm("storage_failure").await;
    intelligence_service.repair_sqlite().await;
}
```

### CRITICAL GAP #4: VersionConflict → audit log + 通知
配置被 cloud 覆盖时写入 audit log，下次 status 上报附带 `config_overridden: true`。

## 错误分类与救援策略

| 分类 | 异常类 | 救援 | 用户可见 |
|------|--------|------|---------|
| 连接 | ConnectionRefused, DnsError | Backoff retry | Log only |
| 连接 | AuthError | 清除凭证→pairing | 需要重新配对 |
| 连接 | MqttDisconnected | OfflineBuffer激活+重连 | Log only |
| 驱动 | DeviceNotFound, DriverNotFound | Skip+mark unhealthy | 设备显示离线 |
| 驱动 | DriverTimeout | Retry 1x, then skip | 属性值过期 |
| 驱动 | LibraryLoadError, SymbolNotFound | 上报cloud | Marketplace显示失败 |
| 存储 | StorageError | Retry 1x+log | 可能数据丢失 |
| 存储 | DiskFullError | 拒绝非alarm写入 | 遥测数据丢失 |
| 存储 | PayloadTooLarge | Truncate+warn | 部分数据丢失 |
| 配置 | ConfigParseError | 保留旧配置 | 配置不变 |
| 配置 | VersionConflict | cloud覆盖 | 本地修改丢失(有audit log) |
| 规则 | RuleCompileError, RuleTimeout | Skip rule+log | 规则暂停 |
| 探针 | ProbePanic | catch_unwind+log | 探针禁用 |
| HTTP | Unauthorized, TooManyRequests | 401/429 | HTTP错误响应 |

## 安全设计

### Secret 管理

| Secret | 存储 | 缓解 |
|--------|------|------|
| MQTT password | `/app/data/credentials.json` (明文) | 文件权限 0600；Phase 3 迁移到 client cert auth |
| EDGE_LOCAL_API_KEY | 环境变量 | 随机生成，启动时读取 |
| 驱动签名公钥 | 编译时嵌入 | 更新公钥需重新部署 |
| 设备连接密码 | `config.yaml` (明文) | 文件权限 0600；Phase 3 加密存储 |

### 威胁缓解

| 威胁 | 缓解 |
|------|------|
| .so 供应链攻击 | SHA256 签名验证 + 文件大小上限 10MB |
| 配对 announce 重放 | TTL 60s + token bucket（已有）+ 建议加 timestamp nonce |
| 配置 YAML 注入 | serde_yaml（safe Rust impl） |
| SQL 注入 | SQLx 参数化查询（已有保证） |
| 本地 HTTP 未授权 | API key Bearer token + 127.0.0.1 binding |

### Audit Logging

结构化日志覆盖所有敏感操作：配对事件、配置变更（来源/版本/时间）、驱动安装/卸载（.so hash/签名结果）、本地 API 调用（endpoint/status/latency）、自愈动作（触发条件/恢复动作/结果）。

## 完整启动流程

```
Edge 启动
    │
    ├── 1. 加载 EdgeConfig（环境变量 + 默认值）
    │
    ├── 2. 检查本地凭证文件 credentials.json
    │       │
    │       ├── 不存在 → Pairing 模式
    │       │   ├── 生成配对码
    │       │   ├── MQTT 匿名连接
    │       │   ├── 定期广播 announce
    │       │   └── 等待 PairingAck → 保存凭证 → 重启
    │       │
    │       └── 存在 ✓
    │           │
    │           ├── 3. 初始化 SQLite（migrate，建新表）
    │           │
    │           ├── 4. 加载本地 config.yaml
    │           │       └── 不存在 → 用默认配置（Scope #1 自治模式）
    │           │
    │           ├── 5. MQTT 认证连接 + 订阅下行 topic
    │           │       ├── 失败 → 重试（指数退避，最大 5min）
    │           │       └── AuthError → 清除凭证 → 回退 Pairing（GAP #1）
    │           │
    │           ├── 6. 从 cloud 拉取最新配置
    │           │       └── 失败 → 继续用本地/默认配置（Scope #1）
    │           │
    │           ├── 7. 启动驱动，首次设备扫描（去重锁保护）
    │           │
    │           ├── 8. 回放离线缓冲
    │           │
    │           ├── 9. 启动 HTTP Server（EDGE_LOCAL_API=1 时）
    │           │
    │           └── 10. 进入主事件循环
    │                   ├── telemetry_tick → 并行采集 + 变换 + 上报
    │                   ├── heartbeat_tick → 状态 + HealthReport
    │                   ├── intelligence_tick → 告警评估 + 自愈探针
    │                   ├── mqtt_msg → 指令 / 配置 / 驱动安装
    │                   └── http_request → 本地 API 处理
    │
    └── 连接断开？
            ├── offline_buffer_active = true
            ├── 遥测+告警写入 SQLite（分级淘汰）
            └── 重连 → 拉配置 → Flush 缓冲 → 恢复正常
```

## 依赖关系图

```
模块合并 (Scope #2)
       │
       ▼
自治模式 (Scope #1)
       │
       ▼
核心运行时 ──── 离线缓冲分级 (Scope #4)
  + config/device topic (Scope #3)
       │
       ├──────────────────────────┐
       ▼                          ▼
驱动动态加载 (Scope #5)    HTTP API (Scope #6)
```

Scope #1-4 之间强依赖。Scope #5 和 #6 相对独立，可并行实施，但都依赖核心运行时就绪。

## 离线→在线状态机

```
                    ┌──────────┐
                    │  ONLINE  │
                    └────┬─────┘
                         │ MQTT disconnect detected
                         ▼
                    ┌──────────┐
                    │ OFFLINE  │──▶ OfflineBuffer.activate()
                    │ BUFFERING│──▶ 遥测+告警 → SQLite
                    └────┬─────┘──▶ 心跳停止
                         │ MQTT reconnect success
                         ▼
                    ┌──────────┐
                    │ FLUSHING │──▶ flush() 每批 500
                    └────┬─────┘──▶ 成功删记录/失败留待下批
                         │ flush complete
                         ▼
                    ┌──────────┐
                    │ SYNCING  │──▶ sync_from_cloud()
                    └────┬─────┘──▶ 可能覆盖本地变更(audit log)
                         │
                         ▼
                    ┌──────────┐
                    │  ONLINE  │ (恢复正常)
                    └──────────┘
```

## 观测性

### Metrics

| Metric | 类型 | 告警阈值 |
|--------|------|---------|
| `edge_mqtt_connected` | Gauge 0/1 | 0 持续 2min→Warning |
| `edge_telemetry_lag_seconds` | Gauge | > 3x interval→Warning |
| `edge_offline_buffer_size` | Gauge | > 50000→Warning, > 90000→Critical |
| `edge_device_count` | Gauge | 突降 50%→Warning |
| `edge_driver_failures` | Counter | rate > 5/min→Critical |
| `edge_cpu_percent` | Gauge | > 90% 5min→Warning |
| `edge_memory_mb` | Gauge | > 80%→Warning |
| `edge_disk_free_mb` | Gauge | < 15%→Warning, < 10%→Critical |

### 结构化日志

每条日志必须带 `device_id`（如有关联设备）和 `action` 字段。使用 tracing span 在入口创建，子调用继承。

## 测试策略

### E2E 场景（5 个硬性要求）

1. 配对→认证→扫描→遥测→心跳（完整 green path）
2. 离线→缓冲→重连→回放（离线恢复 path）
3. Cloud 配置下发→本地合并→驱动重载（配置变更 path）
4. 驱动 .so 安装→验证→加载→扫描（动态加载 path）
5. 首次启动无 cloud（自治模式 path）

### Test Pyramid

Unit tests：每个 service 独立测试（mock 依赖）；Integration tests：service + SQLite + mock MQTT；E2E tests：完整 edge 进程 + mosquitto container。

### 关键 Fixture

- Mock Driver impl（避免依赖真实硬件）
- In-memory SQLite（测试隔离）
- mosquitto test broker container（E2E MQTT）

## 与现有代码的关系

| 文件 | 变更 |
|------|------|
| `edge/src/main.rs` | 重写：事件循环编排 |
| `edge/src/config.rs` | 保留+扩展：增加 telemetry/intelligence/health interval |
| `edge/Cargo.toml` | 添加 4 crate 依赖 + axum + tower + libloading |
| `edge/src/mqtt_client.rs` | 移入 `modules/gateway/service.rs` |
| `edge/src/pairing.rs` | 移入 `modules/gateway/pairing.rs` |
| `edge/src/device_discovery.rs` | 重写为 `modules/driver/service.rs` |
| `edge/src/app_state.rs` | 新增 |
| `edge/src/modules/*/` | 新增 9 个模块 |
| `edge/src/shared/` | 新增（error.rs, storage.rs） |
| `edge/src/modules/http/` | 新增（HTTP server, 12 endpoints） |

## Scope 变更记录（/plan-ceo-review, SCOPE EXPANSION）

| # | Proposal | Effort | Status |
|---|----------|--------|--------|
| 1 | 首次启动自治模式 | S | ACCEPTED |
| 2 | 合并模块 11→9 | S | ACCEPTED |
| 3 | config/device 下行 topic | S | ACCEPTED |
| 4 | 离线缓冲分级淘汰 | S | ACCEPTED |
| 5 | 驱动动态加载 | L | ACCEPTED |
| 6 | 完整本地 HTTP API (12 endpoints) | XL | ACCEPTED |

## Deferred (Phase 3-4)

- OTA 固件更新（A/B 分区、签名验证）
- Edge-to-Edge Mesh
- QR 码配对
- 边缘 AI 推理（TinyML）
- 配置加密存储
- 日志上报到 cloud（`/log` topic）
