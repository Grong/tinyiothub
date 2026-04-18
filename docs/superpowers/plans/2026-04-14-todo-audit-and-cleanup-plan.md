# TinyIoTHub 项目 TODO 清查与清理计划

> 生成时间: 2026-04-14
> 扫描范围: `api/src/`, `web/src/`, `docs/superpowers/`
> 总计 TODO 条目: ~70+

---

## 1. DDD 架构债务 (Phase 3/4 遗留)

**优先级: P1** — 这些直接影响架构合规 (`ARCHITECTURE_HARNESS.md`)

| 文件 | 行号 | TODO 内容 |
|------|------|-----------|
| `api/src/domain/device/service.rs` | 31 | `database` 字段为临时保留，Phase 3 完成后移除 |
| `api/src/domain/device/service.rs` | 65 | 设备创建后加载标签 — 当前 repository 不处理标签 |
| `api/src/domain/device/service.rs` | 117 | `DeviceProperty` 批量创建仍直接依赖 `Database`，应提取到 repository |
| `api/src/domain/device/service.rs` | 136 | 同上，临时使用 `Database` 直接调用 |
| `api/src/domain/device/service.rs` | 213 | 标签处理暂由调用方负责，repository 需扩展 |
| `api/src/domain/device/service.rs` | 401 | `get_device_by_id_with_tags` — repository 当前不处理标签 |
| `api/src/domain/device/service.rs` | 499 | `DeviceProperty` 尚未提取到 repository |
| `api/src/domain/device/service.rs` | 538 | `DeviceCommand` 尚未提取到 repository |
| `api/src/domain/device/service.rs` | 816 | `DeviceCommand` 尚未提取到 repository |
| `api/src/dto/entity/device_command.rs` | 420 | 更新调用者使用 `find_by_device_and_name` 并传入正确的 `Database` 参数 |

**建议行动:**
- 提取 `DevicePropertyRepository` + `DeviceCommandRepository` trait 及 SQLite 实现
- 从 `DeviceService` 移除 `database` escape hatch
- 更新 `AppState` 注入新的 repositories

---

## 2. 告警与事件系统 (Alarm & Event)

**优先级: P1-P2**

| 文件 | 行号 | TODO 内容 |
|------|------|-----------|
| `api/src/api/alarms/events.rs` | 83 | 实现事件触发器创建逻辑 |
| `api/src/api/alarms/events.rs` | 96 | 实现事件触发器详情查询逻辑 |
| `api/src/api/alarms/events.rs` | 109 | 实现事件触发器更新逻辑 |
| `api/src/api/alarms/events.rs` | 121 | 实现事件触发器删除逻辑 |
| `api/src/api/alarms/events.rs` | 133 | 实现事件触发器启用逻辑 |
| `api/src/api/alarms/events.rs` | 145 | 实现事件触发器禁用逻辑 |
| `api/src/api/alarms/rules.rs` | 98 | 实现告警规则创建逻辑 |
| `api/src/api/alarms/rules.rs` | 112 | 实现告警规则详情查询逻辑 |
| `api/src/api/alarms/rules.rs` | 125 | 实现告警规则更新逻辑 |
| `api/src/api/alarms/rules.rs` | 137 | 实现告警规则删除逻辑 |
| `api/src/api/alarms/rules.rs` | 149 | 实现告警规则启用逻辑 |
| `api/src/api/alarms/rules.rs` | 161 | 实现告警规则禁用逻辑 |
| `api/src/domain/alarm/rule.rs` | 47 | Convert to SQLx query (当前可能仍是 raw SQL) |
| `api/src/domain/alarm/services/alarm_service.rs` | 172 | 实现自动解决逻辑 |
| `api/src/domain/alarm/handlers/alarm_event_handler.rs` | 78 | 触发通知 |
| `api/src/api/events/security.rs` | 355 | Add time range filtering when we have proper DateTime parsing |
| `api/src/api/events/security.rs` | 684 | 持久化配置变更到数据库 |

**建议行动:**
- 告警事件触发器 API 为完整 stub，需补全 CRUD + 启停逻辑
- 告警规则 API 同样为 stub，需补全
- 通知触发和自动解决为自愈引擎的关键闭环

---

## 3. 设备驱动与硬件抽象

**优先级: P2-P3**

| 文件 | 行号 | TODO 内容 |
|------|------|-----------|
| `api/src/domain/device/driver/dynamic/wrapper.rs` | 43 | 实现通过 FFI 调用驱动的 `read_data` 方法 |
| `api/src/domain/device/driver/dynamic/wrapper.rs` | 49 | 实现通过 FFI 调用驱动的 `execute_command` 方法 |
| `api/src/domain/device/driver/dynamic/registry.rs` | 146 | 从 `register_drivers!` 宏生成的代码中获取驱动列表 |
| `api/src/domain/device/monitoring_service.rs` | 210 | 实现事件统计 (当前 `total_events = 0u32`) |
| `api/src/domain/plugin/scheduler/handlers/cron.rs` | 18 | 集成 `tokio-cron-scheduler` 实现真正的定时调度 |

**建议行动:**
- FFI wrapper 是动态驱动加载的核心，需要安全封装
- `cron.rs` 目前为空壳，需接入实际调度器

---

## 4. 认证与社交登录

**优先级: P2**

| 文件 | 行号 | TODO 内容 |
|------|------|-----------|
| `api/src/api/auth/session.rs` | 63 | 实现刷新令牌逻辑 |
| `api/src/api/auth/sms.rs` | 148 | 腾讯防水墙 CAPTCHA 验证需要 `app_id` 和 `app_secret` |
| `api/src/api/auth/social.rs` | 350 | 调用微信 API 换取 `access_token` |
| `api/src/api/auth/social.rs` | 409 | 调用微信小程序 API 换取 `session_key` 和 `openid` |
| `api/src/api/auth/social.rs` | 442 | 实现绑定逻辑 |
| `api/src/api/auth/social.rs` | 452 | 实现解绑逻辑 |

**建议行动:**
- 刷新令牌 (refresh token) 是安全合规项
- 微信登录目前为 stub，需要接入微信开放平台真实 API

---

## 5. 监控、日志与系统管理

**优先级: P2-P3**

| 文件 | 行号 | TODO 内容 |
|------|------|-----------|
| `api/src/api/monitoring/logs.rs` | 55 | 实现日志查询逻辑 |
| `api/src/api/monitoring/metrics.rs` | 52 | 实现实际的系统指标收集 |
| `api/src/api/monitoring/metrics.rs` | 70 | 实现设备指标收集 |
| `api/src/api/monitoring/metrics.rs` | 81 | 实现网关指标收集 |
| `api/src/api/monitoring/health.rs` | 37 | 实现实际的运行时间计算 (`uptime_seconds: 0`) |
| `api/src/api/monitoring/health.rs` | 48 | 实现详细健康状态检查逻辑 |
| `api/src/api/system/tasks.rs` | 76 | 实现定时任务查询逻辑 |
| `api/src/api/system/tasks.rs` | 89 | 实现定时任务创建逻辑 |
| `api/src/api/system/tasks.rs` | 114 | 实现定时任务详情查询逻辑 |
| `api/src/api/system/tasks.rs` | 127 | 实现定时任务更新逻辑 |
| `api/src/api/system/tasks.rs` | 139 | 实现定时任务删除逻辑 |
| `api/src/api/system/tasks.rs` | 151 | 实现定时任务启用逻辑 |
| `api/src/api/system/tasks.rs` | 163 | 实现定时任务禁用逻辑 |
| `api/src/api/system/tasks.rs` | 175 | 实现立即运行定时任务逻辑 |
| `api/src/api/system/configuration.rs` | 60 | 从配置文件或数据库读取系统配置 |
| `api/src/api/system/configuration.rs` | 78 | 保存系统配置到配置文件或数据库 |
| `api/src/api/system/configuration.rs` | 109 | 保存网络配置 |
| `api/src/api/system/configuration.rs` | 141 | 保存 MQTT 配置 |
| `api/src/api/system/configuration.rs` | 152 | 实现系统重启逻辑 |
| `api/src/api/system/configuration.rs` | 163 | 实现系统关闭逻辑 |
| `api/src/api/system/features.rs` | 152 | 从配置文件或数据库读取实际的系统功能配置 |
| `api/src/infrastructure/event/sse_manager.rs` | 187 | 实现事件计数器 (`total_events_sent: 0`) |
| `api/src/application/service_manager.rs` | 200 | 实现服务重启逻辑 |

**建议行动:**
- `api/system/tasks.rs` 整模块为 stub，需要基于 `JobService` 实现
- 监控指标/日志/健康检查是生产部署必备

---

## 6. 标签与权限管理

**优先级: P2**

| 文件 | 行号 | TODO 内容 |
|------|------|-----------|
| `api/src/api/tags.rs` | 257 | 实现按类型统计标签 (`by_type: {device, app}`) |
| `api/src/api/users/permissions.rs` | 55 | 实现获取用户权限逻辑 |
| `api/src/api/users/roles.rs` | 181 | 实现获取角色权限逻辑 |
| `api/src/api/users/roles.rs` | 195 | 实现更新角色权限逻辑 |

**建议行动:**
- 权限查询当前为占位符，需要打通 `PermissionService` 与 RBAC

---

## 7. 模板引擎

**优先级: P3**

| 文件 | 行号 | TODO 内容 |
|------|------|-----------|
| `api/src/domain/template/engine.rs` | 509 | 使用 mock 依赖实现真正的单元测试 |
| `api/src/domain/template/repository.rs` | 223 | 实现设备依赖检查 |

---

## 8. 通知通道

**优先级: P2**

| 文件 | 行号 | TODO 内容 |
|------|------|-----------|
| `api/src/dto/entity/notification_channel.rs` | 365 | 实现实际的短信发送 |
| `api/src/dto/entity/notification_channel.rs` | 385 | 实现实际的邮件发送 |
| `api/src/dto/entity/notification_channel.rs` | 413 | 实现实际的 HTTP 请求 |

---

## 9. 网络与系统信息 (Linux / HarmonyOS)

**优先级: P3**

| 文件 | 行号 | TODO 内容 |
|------|------|-----------|
| `api/src/shared/network.rs` | 43 | 实现 Linux 网络脚本初始化 |
| `api/src/shared/network.rs` | 49 | 实现实际的网络信息获取 |
| `api/src/shared/network.rs` | 57 | 实现实际的网络配置 |
| `api/src/shared/network.rs` | 138 | 实现静态 IP 配置 |
| `api/src/shared/network.rs` | 151 | 实现 DHCP 配置 |
| `api/src/shared/network.rs` | 163 | 实现网络服务重启 |
| `api/src/shared/network.rs` | 206 | 实现接口统计信息获取 |
| `api/src/shared/identifier.rs` | 59 | 实现 MAC 地址获取 |
| `api/src/shared/identifier.rs` | 87 | 实现运行时间获取 |
| `api/src/shared/identifier.rs` | 111 | 实现电源状态获取 |
| `api/src/shared/identifier.rs` | 117 | 实现温度读取 |
| `api/src/shared/identifier.rs` | 123 | 实现内存信息获取 |
| `api/src/shared/identifier.rs` | 152 | 实现 CPU 使用率获取 |
| `api/src/shared/identifier.rs` | 158 | 实现磁盘信息获取 |

### HarmonyOS 专用 TODO (大量占位)

| 文件 | 行号 | TODO 内容 |
|------|------|-----------|
| `api/src/infrastructure/hardware/harmonyos/network.rs` | 39-164 | 网络接口枚举、IP设置、启用/禁用、网关设置、ping、统计、刷新 |
| `api/src/infrastructure/hardware/harmonyos/gpio.rs` | 47-128 | GPIO 导出、方向设置、值读写、取消导出 |
| `api/src/infrastructure/hardware/harmonyos/display.rs` | 33-134 | 显示设备初始化、清屏、文本/图像显示、刷新、关闭 |

---

## 10. 前端 TODO

**优先级: P3**

| 文件 | 行号 | TODO 内容 |
|------|------|-----------|
| `web/src/ui/views/chat.ts` | 36 | `agentId = "default"` 应从 URL params 或 store 获取 |

---

## 11. 设计文档中的 TODO (已规划但未实现)

**来源: `docs/superpowers/plans/` 和 `docs/superpowers/specs/`**

| 文档 | 关键 TODO |
|------|-----------|
| `2026-03-26-toml-protocol-plugin.md` | 动态驱动加载（复用 `DynamicDriverLoader`）; PluginHandler → DeviceDriver 转换 |
| `2026-03-28-self-healing-engine.md` | 多租户 `tenant_id` 从 Claims 获取（多处硬编码为 `"default"`）; 实际驱动重启 / LoRa重连 / 设备重连 / 云端上报 / 工单创建 |
| `2026-03-28-self-healing-engine.md` | 查询真实的 `DeviceService` 和 scheduler 状态 |
| `2026-04-10-iot-agent-enhancement.md` | tool spec 安全性查询 |
| `2026-04-11-agent-architecture-redesign.md` | `todo!("Implement chat flow")`; `todo!("Implement get_or_create")`; `todo!("Implement append_message")`; `todo!("Implement get_history")` |
| `2026-04-08-agent-mcp-integration-design.md` | 当 jobs 有 `tenant_id` 时，通过 `claims.tenant_id` 验证租户所有权 |

---

## 建议清理路线图

### Sprint 1: 架构闭环 (P1)
1. 提取 `DevicePropertyRepository` + `DeviceCommandRepository`
2. 从 `DeviceService` 移除 `database` escape hatch
3. 完成 `api/system/tasks.rs`（基于已完成的 `JobService`）

### Sprint 2: 核心功能补全 (P1-P2)
1. 告警事件触发器 + 告警规则 API 完整实现
2. 通知通道：短信、邮件、HTTP webhook 真实发送
3. 刷新令牌 (refresh token) 机制

### Sprint 3: 监控与系统管理 (P2)
1. 日志查询、健康检查、系统指标收集
2. 系统配置持久化到数据库
3. 标签按类型统计

### Sprint 4: 平台适配 (P3)
1. Linux 网络管理脚本实现
2. HarmonyOS 硬件抽象层实现
3. 前端 `chat.ts` 的 `agentId` 动态获取

---

## 维护建议

- **禁止新增裸 TODO**: `ARCHITECTURE_LINTS.md` 已将 `todo = "warn"` 设为默认，建议升级为 `"deny"`
- **TODO 必须带上下文**: 新增 TODO 应包含 `(YYYY-MM-DD, owner)` 和具体下一步行动
- **月度审计**: 每月运行 `rg -n "TODO|todo|fixme|hack|xxx" api/src/ web/src/` 并更新本计划
