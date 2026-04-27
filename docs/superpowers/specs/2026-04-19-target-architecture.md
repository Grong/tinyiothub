# TinyIoTHub 目标架构设计

> 本文档描述 TinyIoTHub 最终应达到的 Workspace 架构。当前重构以此为蓝图，分阶段迁移。

---

## 完整项目结构

```text
tinyiothub/
├── Cargo.toml                          # Workspace 根配置
├── deny.toml                           # cargo-deny 配置
├── Makefile
│
├── crates/                             # 核心库 crate
│   ├── tinyiothub-core/                # 基础类型、常量、零外部框架依赖
│   │   ├── Cargo.toml
│   │   └── src/
│   │       ├── lib.rs
│   │       ├── types.rs                # DeviceId, Timestamp 等 NewType
│   │       ├── constants.rs            # 全局常量
│   │       └── version.rs              # 版本信息
│   │
│   ├── tinyiothub-error/               # 统一错误类型（从 core 拆分）
│   │   ├── Cargo.toml
│   │   └── src/
│   │       ├── lib.rs
│   │       ├── kind.rs                 # ErrorKind 枚举
│   │       └── context.rs              # 错误上下文
│   │
│   ├── tinyiothub-config/              # 配置管理（从 core 拆分）
│   │   ├── Cargo.toml
│   │   └── src/
│   │       ├── lib.rs
│   │       ├── loader.rs
│   │       ├── schema.rs
│   │       └── validation.rs
│   │
│   ├── tinyiothub-metrics/             # 可观测性（新增）
│   │   ├── Cargo.toml
│   │   └── src/
│   │       ├── lib.rs
│   │       ├── meter.rs
│   │       ├── trace.rs
│   │       └── registry.rs
│   │
│   ├── tinyiothub-storage/             # 存储抽象层 + SQLite 实现
│   │   ├── Cargo.toml
│   │   └── src/
│   │       ├── lib.rs
│   │       ├── traits.rs               # Repository traits
│   │       ├── models.rs               # DAO 模型
│   │       └── sqlite/                 # SQLite 实现
│   │           ├── mod.rs
│   │           └── migrations/
│   │
│   ├── tinyiothub-engine/              # 业务引擎核心
│   │   ├── Cargo.toml
│   │   └── src/
│   │       ├── lib.rs
│   │       ├── device/                 # 设备管理
│   │       │   ├── registry.rs
│   │       │   └── shadow.rs
│   │       ├── rule/                   # 规则引擎
│   │       │   ├── parser.rs
│   │       │   ├── evaluator.rs
│   │       │   └── action.rs
│   │       ├── pipeline/               # 数据处理管道
│   │       │   ├── decoder.rs
│   │       │   ├── transformer.rs
│   │       │   └── router.rs
│   │       └── alarm/                  # 告警管理
│   │           ├── trigger.rs
│   │           └── manager.rs
│   │
│   ├── tinyiothub-plugin/              # 插件系统核心
│   │   ├── Cargo.toml
│   │   └── src/
│   │       ├── lib.rs
│   │       ├── ffi.rs                  # C ABI 接口
│   │       ├── loader.rs
│   │       ├── registry.rs
│   │       └── sandbox.rs
│   │
│   ├── tinyiothub-web/                 # Web 层（共享）
│   │   ├── Cargo.toml
│   │   └── src/
│   │       ├── lib.rs
│   │       ├── api/                    # API 路由
│   │       │   ├── mod.rs
│   │       │   ├── v1/
│   │       │   │   ├── mod.rs
│   │       │   │   ├── devices.rs
│   │       │   │   ├── telemetry.rs
│   │       │   │   └── rules.rs
│   │       │   └── dto.rs
│   │       └── middleware/
│   │           ├── auth.rs
│   │           ├── cors.rs
│   │           └── logging.rs
│   │
│   └── tinyiothub-macros/              # 过程宏
│       ├── Cargo.toml
│       └── src/
│           └── lib.rs
│
├── cloud/                              # 云端服务 binary
│   ├── Cargo.toml
│   └── src/
│       ├── main.rs
│       └── server.rs
│
├── edge/                               # 边缘运行时 binary
│   ├── Cargo.toml
│   └── src/
│       ├── main.rs
│       ├── runtime.rs
│       ├── mqtt_client.rs
│       ├── plugin_manager.rs
│       └── offline_storage.rs
│
├── marketplace/                        # 插件市场 binary
│   ├── Cargo.toml
│   └── src/
│       ├── main.rs
│       ├── registry.rs
│       ├── download.rs
│       └── verify.rs
│
├── cli/                                # CLI 工具 binary
│   ├── Cargo.toml
│   └── src/
│       ├── main.rs
│       └── commands/
│
├── plugins/                            # 官方插件实现
│   ├── modbus/
│   ├── onvif/
│   ├── mqtt/
│   └── snmp/
│
├── sdks/                               # SDK
│   └── plugin-sdk/
│
├── vendor/                             # 第三方依赖补丁
│   ├── onvif-rs/
│   └── zeroclaw/
│
├── tests/                              # 集成测试
│   └── integration/
│
├── scripts/                            # 构建/部署脚本
│   ├── build-plugins.sh
│   └── docker-build.sh
│
├── deploy/                             # 部署配置
│   ├── docker/
│   └── config/
│
└── docs/                               # 文档
    ├── architecture.md
    └── api/
```

## 关键设计决策

### 1. Core 零框架依赖原则
- `tinyiothub-core` 不依赖 tokio、axum、sqlx、thiserror 等框架
- 只保留 serde + chrono + uuid（基础序列化）
- 错误类型使用手动 impl std::error::Error，不用 thiserror derive

### 2. Error 独立拆分
- 从 core 中拆分出 `tinyiothub-error`，允许使用 thiserror
- core 定义基础错误变体，error crate 提供完整错误链

### 3. Storage 只暴露 traits
- Repository traits 定义在 storage crate
- 具体实现（SQLite/PostgreSQL/内存）作为子模块
- engine 只依赖 storage::traits，不依赖具体实现

### 4. Web 层独立
- HTTP handlers、middleware、路由定义在 `tinyiothub-web`
- cloud 和 edge binary 都可以复用 web 层
- cloud 只负责：main.rs（配置加载 + server 启动）

### 5. 依赖方向（强制）
```
cloud/edge/marketplace/cli → tinyiothub-web → tinyiothub-engine → tinyiothub-storage → tinyiothub-error → tinyiothub-core
plugins/ → tinyiothub-plugin-sdk → tinyiothub-core
```

禁止反向依赖。

## 当前状态差距

| 目标 | 当前 | 差距 |
|------|------|------|
| core 零依赖 | core 含 thiserror + sqlx(optional) | 需要移除 these deps |
| error 独立 crate | error 在 core 内部 | 需要拆分 |
| config 独立 crate | config 在 core 内部 | 需要拆分 |
| engine 含 domain | engine 为空，domain 在 cloud/ | 需要提取 |
| storage 含 repo traits | storage 为空，repo 在 cloud/ | 需要提取 |
| web 独立 crate | web 不存在，handlers 在 cloud/ | 需要创建 |
| cloud 仅 binary | cloud 含 lib + domain + api | 需要瘦身 |
