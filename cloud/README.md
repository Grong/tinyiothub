# TinyIoTHub API - Rust Backend

基于 Rust 的云端 SaaS 物联网后端服务，支持配置和管理边缘网关设备。

## 目录结构

```
api/
├── src/                      # Rust 源代码
│   ├── api/                  # REST API 层
│   │   ├── auth/             # 认证相关 API
│   │   ├── devices/          # 设备管理 API
│   │   ├── drivers/          # 驱动管理 API
│   │   ├── alarms/           # 告警管理 API
│   │   ├── alarm_rules/      # 告警规则 API
│   │   ├── agents/           # AI Agent 管理 API
│   │   ├── chat/             # AI Agent 聊天 API
│   │   ├── events/           # 事件管理 API
│   │   ├── jobs/             # 定时任务 API
│   │   ├── marketplace/      # 应用市场 API
│   │   ├── mcp/              # 内嵌 MCP Server
│   │   ├── notifications/    # 通知管理 API
│   │   ├── notification_channels/ # 通知渠道 API
│   │   ├── self_healing/     # 自愈引擎 API
│   │   ├── system/           # 系统管理 API
│   │   ├── monitoring/       # 监控 API
│   │   ├── templates/        # 设备模板 API
│   │   ├── tenants/          # 租户管理 API
│   │   ├── users/            # 用户管理 API
│   │   ├── workspaces/       # 工作空间 API
│   │   ├── batch/            # 批量操作 API
│   │   ├── open/             # 开放接口 API
│   │   ├── heartbeat/        # 心跳 API
│   │   └── middleware/       # 中间件
│   ├── application/          # 应用服务层
│   │   ├── agent/            # Agent 会话、聊天、记忆服务
│   │   ├── cron_scheduler.rs # 定时任务调度（CronSchedulerService）
│   │   ├── data_context.rs   # 数据上下文
│   │   ├── data_server.rs    # 数据服务
│   │   └── service_manager.rs # 服务管理器
│   ├── domain/               # 领域层
│   │   ├── agent/            # Agent 领域
│   │   ├── alarm/            # 告警领域
│   │   ├── automation/       # 自动化领域
│   │   ├── cron/             # 定时任务领域
│   │   ├── device/           # 设备领域（含 driver/registry）
│   │   ├── event/            # 事件领域
│   │   ├── marketplace/      # 市场领域
│   │   ├── permission/       # 权限领域
│   │   ├── plugin/           # 插件领域
│   │   ├── product/          # 产品领域
│   │   ├── role/             # 角色领域
│   │   ├── self_healing/     # 自愈引擎领域
│   │   ├── tag/              # 标签领域
│   │   ├── template/         # 模板领域
│   │   ├── tenant/           # 租户领域
│   │   ├── user/             # 用户领域
│   │   └── workspace/        # 工作空间领域
│   │       ├── repository.rs # Repository trait（接口）
│   │       └── service.rs    # 领域服务
│   ├── dto/                  # 数据传输对象（纯结构体，无 SQL）
│   ├── infrastructure/       # 基础设施层
│   │   └── persistence/      # 数据持久化
│   │       └── repositories/ # Repository 实现（SQLite）
│   ├── shared/               # 共享组件
│   ├── lib.rs                # 库入口
│   └── main.rs               # 程序入口
├── derive/                   # 自定义宏
├── migrations/               # 数据库迁移文件
├── drivers/                  # 驱动实现
├── templates/                # 设备模板
├── vendor/                   # 第三方依赖（本地 fork）
├── Cargo.toml                # 项目配置
├── Dockerfile                # Docker 构建文件
├── app_settings.toml         # 应用配置
└── tinyiothub.db             # SQLite 数据库
```

## 快速开始

### 开发运行

```bash
cd api
cargo run
```

### 发布构建

```bash
cd api
cargo build --release
```

### 运行测试

```bash
cd api
cargo test
```

## 配置

主配置文件: `app_settings.toml`

```toml
[server]
host = "0.0.0.0"
port = 3002

[database]
url = "tinyiothub.db"
auto_migrate = true

[mqtt.primary]
host = "192.168.1.124"
port = 1883
```

## API 端点

服务启动后访问: http://localhost:3002/api/v1/

主要端点:
- `/api/v1/system/health` - 健康检查
- `/api/v1/auth/login` - 用户登录
- `/api/v1/devices` - 设备管理
- `/api/v1/drivers` - 驱动管理
- `/api/v1/templates` - 模板管理
- `/api/v1/alarms` - 告警管理
- `/api/v1/alarm-rules` - 告警规则
- `/api/v1/agents` - AI Agent 管理
- `/api/v1/agents/skills` - Agent 技能调用
- `/api/v1/workspaces` - 工作空间
- `/api/v1/jobs` - 定时任务
- `/api/v1/self-healing` - 自愈引擎
- `/api/v1/events` - 事件查询
- `/api/v1/notifications` - 通知管理

## 技术栈

- **Rust 2024 Edition**
- **Axum** - Web 框架
- **Tokio** - 异步运行时
- **SQLx** - 数据库访问
- **SQLite** - 数据存储
- **rumqttc** - MQTT 客户端

## 开发指南

详细开发指南请参考项目根目录的文档:
- [技术栈规范](../.kiro/steering/tech.md)
- [项目结构](../.kiro/steering/structure.md)
- [API 开发规范](../.kiro/steering/api-standards.md)
- [命名规范](../.kiro/steering/naming.md)
