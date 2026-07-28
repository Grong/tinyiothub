# TinyIoTHub API - Rust Backend

基于 Rust 的云端 SaaS 物联网后端服务，支持配置和管理边缘网关设备。

## 目录结构

```
cloud/
├── src/
│   ├── api/                  # 路由挂载 + HTTP 中间件（WorkspaceScope, auth）
│   ├── modules/              # 业务模块（types → service → handler 三层结构）
│   │   ├── thing/            # 物本体管理（CRUD、层级树、本体、资源、LLM 摘要）
│   │   ├── device/           # 设备连接运行时（驱动、遥测、心跳）
│   │   ├── event/            # 事件管道（router、throttle、real-time、SSE、保留任务）
│   │   ├── alarm/            # 告警规则 + 通知
│   │   ├── agent/            # AI Agent（chat、config、tools、session、memory）
│   │   ├── template/         # 物模板（创建时蓝图）
│   │   ├── marketplace/      # 应用市场（驱动 / 物模板）
│   │   ├── workspace/        # 工作空间（含知识资源）
│   │   ├── mcp/              # 内嵌 MCP Server
│   │   └── ...               # auth, chat, cron, jobs, open, system 等
│   ├── shared/               # 跨模块组件（persistence, security, error_handling, utils）
│   ├── tests/                # 集成测试
│   ├── lib.rs                # 库入口
│   └── main.rs               # 程序入口
├── migrations/               # 数据库迁移文件
├── templates/                # 物模板
└── Cargo.toml                # 项目配置
```

## 快速开始

### 开发运行

```bash
cd cloud
cargo run
```

### 发布构建

```bash
cd cloud
cargo build --release
```

### 运行测试

```bash
cd cloud
cargo test
```

## 配置

主配置文件: 仓库根目录 `app_settings.toml`

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
- `/api/v1/things` - 物管理（设备/空间/产线统一模型）
- `/api/v1/drivers` - 驱动管理
- `/api/v1/device-templates` - 物模板管理
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
