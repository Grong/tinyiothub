# TinyIoTHub 架构重构设计文档

## 概述

**问题诊断**：当前代码库存在命名不规范、结构混乱、冗余重复、设计模式缺失等问题，整体呈现"小学生水平"，缺乏专业性。

**重构目标**：彻底重构整个workspace，建立清晰、专业、符合Rust最佳实践的架构，达到生产级代码质量。

**设计原则**：
1. **模块化架构** - 按技术职责划分crate，每个crate内聚且职责清晰
2. **基于职责命名** - 拒绝泛化的`*Service`、`*Manager`命名，使用表达具体职责的名称
3. **简洁表达** - 命名直接、简洁，避免不必要的后缀
4. **严格边界** - crate之间清晰的依赖方向和职责隔离

## 架构设计

### 1. Crate 结构（6个crate）

```
tinyiothub/
├── Cargo.toml (workspace)
├── crates/
│   ├── tinyiothub-types/      # 纯领域类型（零依赖）
│   ├── tinyiothub-storage/    # 数据存取抽象与实现
│   ├── tinyiothub-engine/     # IoT 业务引擎（纯逻辑）
│   ├── tinyiothub-api/        # HTTP 接口与中间件
│   └── tinyiothub-shared/     # 共享工具（错误、工具、扩展）
└── cloud/ → tinyiothub-cloud/ # SaaS 主应用（重命名目录）
```

### 2. Crate 职责边界

#### `tinyiothub-types`（类型层）
**职责**：纯数据类型定义，零业务逻辑

**✅ 允许**：
- `Device`、`Event`、`TelemetryPoint`、`DeviceId` 等结构体和枚举
- `serde`、`chrono`、`uuid` 序列化注解
- 简单的数据验证方法（如 `validate()`）

**❌ 禁止**：
- 任何业务逻辑方法
- `sqlx::FromRow`、`diesel` 等数据库映射
- HTTP框架依赖（`axum::Json` 等）
- SaaS模型（`Tenant`、`User`、`Workspace`）

#### `tinyiothub-storage`（存储层）
**职责**：数据存取抽象与实现

**✅ 允许**：
- `DeviceRepository`、`TelemetryRepository` trait定义
- SQLite/PostgreSQL具体实现
- 查询构建器、分页支持
- 内存缓存（不感知租户）

**❌ 禁止**：
- `WHERE workspace_id = ?` 查询条件
- SaaS仓储trait（`TenantRepository`、`UserRepository`）
- 业务逻辑（告警计算、规则评估等）
- 反向依赖`cloud`或`api`

#### `tinyiothub-engine`（引擎层）
**职责**：纯IoT业务逻辑处理

**✅ 允许**：
- 设备采集引擎、驱动注册表
- 规则引擎、告警计算、自动化逻辑
- 模板渲染、自愈引擎
- 通用调度器（不包含租户信息）

**❌ 禁止**：
- 多租户逻辑（不处理`workspace_id`）
- HTTP框架依赖（`axum`、`hyper`、`tower`）
- 用户权限检查、租户隔离
- SaaS服务（市场、代理、通知推送）

#### `tinyiothub-api`（接口层）
**职责**：通用HTTP基础设施

**✅ 允许**：
- 通用HTTP handler（`GET /devices`、`POST /telemetry`）
- 中间件（`RequestId`、`Logging`、`Metrics`、`Cors`）
- `ApiResponseBuilder`、请求验证、DTO定义
- WebSocket、SSE支持

**❌ 禁止**：
- 租户鉴权中间件（JWT解析`workspace_id`）
- SaaS API路由（`/workspaces`、`/users`、`/marketplace`）
- 反向依赖`cloud` crate

#### `tinyiothub-shared`（共享层）
**职责**：跨crate共享组件

**✅ 允许**：
- **错误定义**：`Error`枚举、`Result`类型别名、错误转换
- **通用工具**：密码哈希、时间处理、配置解析、密码学工具
- **类型扩展**：常见trait扩展（`StringExt`、`OptionExt`）、过程宏

**依赖**：最小化基础库，可被所有层依赖

#### `tinyiothub-cloud`（应用层）
**职责**：SaaS应用编排与租户逻辑

**✅ 允许**：
- SaaS领域模型（`Tenant`、`User`、`Workspace`、`Role`、`Permission`）
- 租户感知适配器（`TenantDeviceRepository`包装器）
- SaaS专属路由（`/api/v1/workspaces/*`、`/api/v1/users/*`）
- 服务编排、依赖注入、应用启动
- 依赖所有内部crate

**❌ 禁止**：
- 将SaaS逻辑下沉到通用crate
- 污染通用DTO（在`api`的DTO中直接嵌入`workspace_id`）

### 3. 依赖方向

```
tinyiothub-cloud (应用层)
        ↓
    tinyiothub-api (接口层)
        ↓
    tinyiothub-engine (引擎层)
        ↓
    tinyiothub-storage (存储层)
        ↓
    tinyiothub-types (类型层)
        ↑
    tinyiothub-shared (共享层，可被所有层依赖)
```

**关键约束**：
- 严格单向依赖，禁止循环依赖
- `types`完全独立，零基础设施依赖
- `cloud`可以依赖所有内部crate
- `shared`作为工具层，可被所有crate依赖

### 4. 命名规范（基于职责，简洁表达）

| 职责场景 | 命名模式 | 示例 | 说明 |
|----------|----------|------|------|
| 数据存取 | `*Repository` | `DeviceRepository`、`TelemetryRepository` | 数据存取抽象 |
| 业务处理 | `*Processor` | `DeviceCommandProcessor`、`AlarmEventProcessor` | 核心业务逻辑处理 |
| 流程协调 | `*Coordinator` | `DataCollectionCoordinator`、`WorkflowCoordinator` | 跨组件流程协调 |
| 请求处理 | `*Handler` | `DeviceQueryHandler`、`TenantCreationHandler` | HTTP请求处理器 |
| 引擎核心 | `*Engine` | `RuleEngine`、`TemplateEngine` | 业务规则引擎 |
| 状态管理 | 简洁名词 | `Connection`、`Cache`、`Session`、`Pool` | 基础设施状态管理 |

**淘汰的泛化命名**：
- ❌ `*Service` - 含义模糊，改为具体职责（`Processor`、`Coordinator`等）
- ❌ `*Manager` - 除非确为复杂状态管理，否则使用简洁名词
- ❌ `*Server` - 除非是真的服务器实现（如`MqttServer`）
- ❌ 无意义的 `*Impl` - 通过模块组织（`sqlite::`、`postgres::`）

**命名示例优化**：
- `DeviceService` → `DeviceCommandProcessor`（处理设备命令）
- `UserManager` → `UserAuthenticator`（用户认证）
- `CacheManager` → `Cache`（缓存管理）
- `ConnectionManager` → `ConnectionPool`（连接池）
- `AlarmService` → `AlarmEventProcessor`（告警事件处理）

### 5. 代码质量标准

#### 测试策略
- **单元测试**：每个公开函数，`tests/`目录与源码同结构
- **集成测试**：跨crate功能，`tests/integration/`目录
- **属性测试**：关键业务规则使用`proptest`验证不变量
- **基准测试**：关键路径使用`criterion`进行性能测试

#### 代码规范
- `clippy::pedantic` + `rustfmt` 强制执行
- 文档注释覆盖率 ≥80%（`cargo doc`检查）
- 复杂算法必须包含`// Algorithm:`或`// Safety:`注释
- 错误处理：使用`thiserror`，提供可读错误信息

#### 性能要求
- 关键API P95延迟 ≤200ms，P99 ≤500ms
- 内存使用：24小时运行增长 ≤10MB
- 数据库查询：N+1问题零容忍，批量操作支持
- 并发安全：正确标记`Send` + `Sync`，避免死锁

## 实施路线图（6周）

### 第一周：基础架构搭建
1. 创建新workspace，搭建6个crate结构
2. 迁移`tinyiothub-types`（从core提取纯净类型）
3. 建立`tinyiothub-shared`（错误定义+通用工具）
4. 配置CI/CD流水线（clippy、fmt、test、coverage）

**交付物**：可编译的基础crate结构，CI/CD通过

### 第二周：存储层重构
1. 重写`tinyiothub-storage`，彻底移除租户污染
2. 实现干净的Repository trait和SQLite实现
3. 建立数据库迁移分离（IoT表 vs SaaS表）
4. 存储层测试套件（单元+集成，≥90%覆盖率）

**交付物**：纯净的存储层，零`workspace_id`引用

### 第三周：业务引擎重构
1. 重构`tinyiothub-engine`，提取纯业务逻辑
2. 实现设备采集、规则引擎、告警计算
3. 移除所有HTTP和租户依赖
4. 引擎集成测试（模拟设备通信，完整场景）

**交付物**：独立的业务引擎，可通过trait测试

### 第四周：API层重构
1. 创建`tinyiothub-api`，提取通用HTTP设施
2. 实现`ApiResponseBuilder`、中间件、DTO
3. 移除租户鉴权逻辑到cloud
4. API契约测试（OpenAPI生成，接口验证）

**交付物**：通用的HTTP基础设施层

### 第五周：SaaS应用重构
1. 重构`tinyiothub-cloud`，实现租户感知适配器
2. 重建SaaS领域模型（tenant、user、workspace）
3. 实现服务编排和依赖注入
4. 端到端测试完整SaaS流程（注册→租户→设备）

**交付物**：完整的SaaS应用，支持多租户

### 第六周：迁移验证
1. 渐进式迁移，保持API兼容性（兼容层）
2. 性能基准测试和优化（关键路径分析）
3. 安全审计（OWASP Top 10检查，依赖扫描）
4. 文档更新和团队交接（架构图、开发指南）

**交付物**：生产就绪的完整系统

## 成功验证指标

### 1. 架构合规性
```bash
# 存储层零租户污染
grep -r "workspace_id\|tenant_id" crates/tinyiothub-storage/src  # 期望：无输出

# 类型层零基础设施依赖
cargo tree -p tinyiothub-types --edges normal | grep -E "(tokio|axum|sqlx)"  # 期望：无输出

# 引擎层零HTTP依赖
cargo tree -p tinyiothub-engine --edges normal | grep -E "(axum|hyper|tower)"  # 期望：无输出
```

### 2. 代码质量
```bash
# 零警告
cargo clippy --workspace -- -D warnings  # 期望：通过

# 格式化检查
cargo fmt -- --check  # 期望：通过

# 测试覆盖率
cargo tarpaulin --workspace --out Html  # 期望：≥85%行覆盖率

# 所有测试通过
cargo test --workspace  # 期望：全部通过
cargo test --release  # 期望：发布模式通过
```

### 3. 功能正确性
- **单元测试**：每个公开函数至少1个正常路径+1个错误路径测试
- **集成测试**：跨crate业务流程测试（设备注册→采集→告警→通知）
- **API测试**：所有HTTP端点测试（正常+异常+边界条件）
- **并发测试**：高并发场景测试（100+并发设备连接）

### 4. 性能指标
- **API延迟**：关键API P95 ≤200ms，P99 ≤500ms（95%分位数≤200ms，99%分位数≤500ms）
- **内存使用**：24小时压测内存增长 ≤10MB
- **数据库连接**：连接池无泄漏，连接数稳定
- **吞吐量**：支持≥1000设备并发连接，≥10000 QPS遥测写入

## 风险与缓解

| 风险 | 影响 | 缓解措施 |
|------|------|----------|
| 迁移周期长 | 新功能开发延迟 | 分阶段交付，每阶段都有可运行版本，保持业务连续性 |
| API兼容性破坏 | 前端需要适配 | 提供兼容层（3个月弃用期），渐进式迁移 |
| 测试覆盖率下降 | 质量风险 | 测试驱动开发，覆盖率门禁（<85%阻塞合并） |
| 性能回归 | 用户体验下降 | 每个阶段性能基准测试，性能监控告警 |
| 团队学习成本 | 开发效率降低 | 详细文档+示例代码+代码审查+知识分享会 |
| 数据迁移风险 | 数据丢失或损坏 | 双写双读迁移，数据验证脚本，回滚计划 |

## 技术决策记录

### 1. 为什么选择6个crate？
- **职责清晰**：每个crate单一职责，便于理解和维护
- **编译优化**：独立编译，增量构建更快
- **复用性**：`types`、`storage`、`engine`可独立用于非SaaS场景
- **团队协作**：不同团队可并行开发不同crate

### 2. 为什么重命名`cloud`为`tinyiothub-cloud`？
- **一致性**：与其他crate命名风格一致
- **清晰性**：明确是SaaS云端版本，与可能的`tinyiothub-edge`对应
- **工具支持**：cargo工具更好处理统一前缀

### 3. 为什么使用基于职责的命名？
- **表达力**：从名称就能理解代码职责
- **可发现性**：搜索`*Processor`找到所有业务逻辑处理
- **重构友好**：职责变更时名称自然需要变更，减少隐藏的技术债务

### 4. 为什么保持`tinyiothub-`前缀？
- **品牌识别**：明确是TinyIoTHub项目
- **命名空间**：避免与系统库或其他crate冲突
- **发布友好**：crates.io上统一前缀便于查找

## 下一步行动

1. ✅ 用户审查本设计文档
2. 🔄 根据反馈调整设计
3. 🔄 编写详细实施计划（使用`/writing-plans`技能）
4. 🔄 开始第一阶段实施

---

**设计状态**：草案 v1.0  
**设计者**：Claude Code  
**最后更新**：2026-04-22  
**关联文档**：`ARCHITECTURE_CONTRACT.md`、`ARCHITECTURE_HARNESS.md`