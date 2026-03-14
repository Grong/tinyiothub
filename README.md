# TinyIoTHub - Rust Implementation

**版本**: v1.0.0  
**官方网站**: https://tinyiothub.com  
**仓库地址**: https://github.com/Grong/tinyiothub  
**Docker Hub**: https://hub.docker.com/r/chenguorongz/tinyiothub  
**发布日期**: 2026-01-19

基于 Rust 的物联网边缘网关系统，专为鸿蒙系统优化，支持多种设备协议和数据采集。

## 版本说明

本项目基于 Rust 2021 Edition，针对鸿蒙系统进行了优化。

## 特性

- 🚀 **高性能异步架构**（基于 Tokio）
- 🔌 **多协议支持**（Modbus RTU/TCP、ONVIF、SNMP、Ping）
- 📊 **实时数据采集和处理**
- 🌐 **现代化 REST API**（基于 Axum + 统一响应格式）
- 📱 **MQTT 消息推送**（支持主备双通道）
- 🔐 **JWT 身份认证**（支持会话管理）
- 📈 **设备监控和告警**（实时状态监控）
- 🎯 **事件驱动架构**（设备联动和规则引擎）
- 💾 **SQLite 数据存储**（支持自动迁移）
- 🔄 **自动重连和故障恢复**
- 🤖 **鸿蒙系统原生支持**
- ⚙️ **专业配置系统**（层次化配置，环境感知）
- 🔒 **安全加固**（配置验证，权限控制）
- 🎨 **现代化前端界面**（Next.js + TypeScript + TailwindCSS）

## 项目结构

```
tinyiothub/
├── api/                      # Rust 后端服务
│   ├── src/                  # 源代码
│   ├── migrations/           # 数据库迁移
│   ├── drivers/              # 驱动实现
│   ├── templates/            # 设备模板
│   ├── vendor/               # 第三方依赖
│   ├── Cargo.toml            # Rust 项目配置
│   └── tinyiothub.db         # SQLite 数据库
├── web/                      # Next.js 前端应用
│   ├── app/                  # 页面和组件
│   ├── service/              # API 服务层
│   └── package.json          # Node.js 项目配置
├── sdks/                     # SDK 开发包
│   └── driver-sdk/           # 驱动开发 SDK
├── examples/                 # 示例项目
│   ├── example-plugin/       # 插件示例
│   └── bacnet-driver/        # BACnet 驱动示例
├── marketplace/              # 市场资源
│   ├── drivers/              # 驱动市场
│   └── templates/            # 模板市场
├── scripts/                  # 工具脚本
├── docs/                     # 项目文档
└── .kiro/                    # 开发规范
```

## 快速开始

### 环境要求

**后端**:
- **Rust**: 1.70+ (2021 Edition)
- **操作系统**: Linux, Windows, HarmonyOS
- **数据库**: SQLite (内置)
- **网络**: MQTT Broker (可选)

**前端**:
- **Node.js**: 18+
- **pnpm**: 8+ (推荐包管理器)
- **浏览器**: Chrome, Firefox, Safari, Edge

### 安装和运行

#### 开发模式（分离部署）

**后端**:
```bash
cd api
cargo run
```

**前端**:
```bash
cd web
pnpm install
pnpm dev
```

访问: http://localhost:3001

#### 生产模式（单进程部署）

**构建**:
```bash
# Windows
.\scripts\build-single-binary.ps1 -Release

# Linux/macOS
./scripts/build-single-binary.sh --release
```

**运行**:
```bash
cd api
.\target\release\tinyiothub.exe  # Windows
./target/release/tinyiothub      # Linux/macOS
```

访问: http://localhost:3002

**优势**:
- ✅ 单进程部署，无需 Node.js
- ✅ 内存占用低（~80MB vs ~200MB）
- ✅ 启动快速（<2s vs ~5s）
- ✅ 支持动态路由

详见: [单进程部署方案](docs/deployment/single-process-deployment.md)
cd web

# 安装依赖
pnpm install

# 开发运行
pnpm dev

# 构建生产版本
pnpm build

# 启动生产服务器
pnpm start
```

### 配置文件

后端配置文件位于 `api/app_settings.toml`：

```toml
# api/app_settings.toml 示例
[server]
host = "0.0.0.0"
port = 3002

[database]
url = "tinyiothub.db"
auto_migrate = true

[mqtt.primary]
host = "192.168.1.124"
port = 1883
username = "admin"
password = "password"

[security.jwt]
secret = "your-secret-key-must-be-at-least-32-characters-long"
expiration_secs = 10800  # 3 hours
```

前端配置文件位于 `web/.env.local`：

```env
NEXT_PUBLIC_API_URL=http://localhost:3002
```

### 访问服务

启动后访问以下地址：

- **Web 管理界面**: http://localhost:3001/ (前端开发服务器)
- **后端 API**: http://localhost:3002/api/v1/
- **健康检查**: http://localhost:3002/api/v1/system/health

## API 开发规范

本项目严格遵循统一的API开发规范，确保前后端数据对接的一致性。

### 统一响应格式

所有API端点必须返回以下格式：

```json
{
    "code": 0,           // 0表示成功，非0表示错误
    "msg": "",           // 错误信息，成功时为空字符串
    "result": T | null   // 实际数据，错误时为null
}
```

### 后端API规范

```rust
// ✅ 正确的API函数签名
async fn list_devices(
    Query(params): Query<DeviceQuery>,
    State(state): State<AppState>,
) -> Json<ApiResponse<Vec<Device>>> {
    // 业务逻辑
    let devices = get_devices(&params).await?;
    ApiResponseBuilder::success(devices)
}

// 使用统一的响应构建器
use crate::dto::response::builder::ApiResponseBuilder;

// 成功响应
ApiResponseBuilder::success(data)

// 错误响应
ApiResponseBuilder::error("错误信息")
```

### 前端API调用规范

```typescript
// ✅ 正确：使用统一API客户端
import { apiGet, apiPost, apiPut, apiDelete } from '@/lib/api-client'

// GET请求
const response = await apiGet<UserList>('users', { page: 1, pageSize: 20 })

// POST请求
const response = await apiPost<User>('users', userData)
```

### Service层结构

```typescript
// web/service/users.ts
export const userApi = {
  getUsers: (params?: { page?: number; pageSize?: number }) => 
    apiGet<User[]>('users', params),
  createUser: (data: CreateUserRequest) => 
    apiPost<User>('users', data),
}

// React Query Hooks
export const useUsers = (params?: { page?: number; pageSize?: number }) => {
  return useQuery({
    queryKey: queryKeys.users.list(params || {}),
    queryFn: async () => {
      const response = await userApi.getUsers(params)
      return response.result || []
    },
  })
}
```

详细的API开发规范请参考：[API开发规范](.kiro/steering/api-standards.md)

## 项目架构

### 整体架构

```
┌─────────────────┐    ┌─────────────────┐    ┌─────────────────┐
│   Next.js UI    │    │   REST API      │    │   MQTT Client   │
│   (web/)        │    │   (api/)        │    │   (rumqttc)     │
└─────────────────┘    └─────────────────┘    └─────────────────┘
         │                       │                       │
         └───────────────────────┼───────────────────────┘
                                 │
         ┌─────────────────────────────────────────────────────┐
         │              Application Layer                      │
         │  ┌─────────────┐  ┌─────────────┐  ┌─────────────┐ │
         │  │ Data Server │  │Message Server│  │  Scheduler  │ │
         │  └─────────────┘  └─────────────┘  └─────────────┘ │
         └─────────────────────────────────────────────────────┘
                                 │
         ┌─────────────────────────────────────────────────────┐
         │               Domain Layer                          │
         │  ┌─────────────┐  ┌─────────────┐  ┌─────────────┐ │
         │  │   Device    │  │    Alarm    │  │    Event    │ │
         │  │   Domain    │  │   Domain    │  │   Domain    │ │
         │  └─────────────┘  └─────────────┘  └─────────────┘ │
         └─────────────────────────────────────────────────────┘
                                 │
         ┌─────────────────────────────────────────────────────┐
         │            Infrastructure Layer                     │
         │  ┌─────────────┐  ┌─────────────┐  ┌─────────────┐ │
         │  │   Config    │  │  Hardware   │  │ Persistence │ │
         │  │  System     │  │ Abstraction │  │   (SQLite)  │ │
         │  └─────────────┘  └─────────────┘  └─────────────┘ │
         └─────────────────────────────────────────────────────┘
```

### 后端目录结构 (api/)

```
api/
├── src/
│   ├── api/                  # REST API 层
│   │   ├── auth/             # 认证相关 API
│   │   ├── devices/          # 设备管理 API
│   │   ├── drivers/          # 驱动管理 API
│   │   ├── alarms/           # 告警管理 API
│   │   ├── users/            # 用户管理 API
│   │   ├── system/           # 系统管理 API
│   │   ├── monitoring/       # 监控 API
│   │   ├── templates/        # 设备模板 API
│   │   └── middleware/       # 中间件
│   ├── application/          # 应用服务层
│   ├── domain/               # 领域层
│   ├── dto/                  # 数据传输对象
│   ├── infrastructure/       # 基础设施层
│   ├── shared/               # 共享组件
│   └── main.rs               # 程序入口
├── migrations/               # 数据库迁移
├── drivers/                  # 驱动实现
├── Cargo.toml                # Rust 项目配置
└── app_settings.toml         # 应用配置
```

### 前端目录结构 (web/)

```
web/
├── app/                      # Next.js App Router
│   ├── components/           # React 组件
│   │   ├── base/             # 基础组件
│   │   ├── devices/          # 设备相关组件
│   │   ├── templates/        # 模板相关组件
│   │   └── workflow/         # 工作流组件
│   ├── (dashboard)/          # 仪表板页面
│   └── globals.css           # 全局样式
├── lib/                      # 工具库
│   ├── api-client.ts         # 统一API客户端
│   └── query-keys.ts         # React Query 键管理
├── service/                  # API服务层
│   ├── auth.ts               # 认证服务
│   ├── devices.ts            # 设备服务
│   └── drivers.ts            # 驱动服务
├── types/                    # TypeScript 类型定义
└── package.json              # 项目配置
```

## API 接口

### 认证接口
- `POST /api/v1/auth/login` - 用户登录
- `POST /api/v1/auth/logout` - 用户登出
- `GET /api/v1/auth/session` - 获取会话信息

### 设备管理
- `GET /api/v1/devices` - 获取设备列表
- `POST /api/v1/devices` - 创建设备
- `GET /api/v1/devices/{id}` - 获取设备详情
- `PUT /api/v1/devices/{id}` - 更新设备
- `DELETE /api/v1/devices/{id}` - 删除设备
- `GET /api/v1/devices/{id}/profile` - 获取设备配置文件

### 驱动管理
- `GET /api/v1/drivers` - 获取驱动列表
- `GET /api/v1/drivers/{name}` - 获取驱动详情
- `GET /api/v1/drivers/{name}/config` - 获取驱动配置参数
- `GET /api/v1/drivers/names` - 获取支持的驱动名称

### 设备模板
- `GET /api/v1/device-templates` - 获取模板列表
- `GET /api/v1/device-templates/{id}` - 获取模板详情
- `GET /api/v1/device-templates/categories` - 获取模板分类
- `POST /api/v1/device-templates/{id}/validate` - 验证模板输入
- `POST /api/v1/device-templates/{id}/preview` - 预览设备创建

### 告警管理
- `GET /api/v1/alarms` - 获取告警列表
- `POST /api/v1/alarms/{id}/acknowledge` - 确认告警
- `GET /api/v1/alarms/rules` - 获取告警规则

### 用户管理
- `GET /api/v1/users` - 获取用户列表
- `POST /api/v1/users` - 创建用户
- `GET /api/v1/users/roles` - 获取角色列表

### 系统管理
- `GET /api/v1/system/health` - 健康检查
- `GET /api/v1/system/features` - 获取系统特性
- `GET /api/v1/system/config` - 获取系统配置

### 监控接口
- `GET /api/v1/monitoring/health` - 健康检查
- `GET /api/v1/monitoring/metrics` - 系统指标
- `GET /api/v1/monitoring/dashboard/stats` - 仪表板统计

## 开发指南

### 后端开发

#### 添加新API端点

1. 在相应的API模块中创建处理函数
2. 使用统一的响应构建器
3. 遵循命名规范

```rust
// 示例：添加新API
use crate::dto::response::builder::ApiResponseBuilder;

async fn list_items(
    Query(params): Query<ItemQuery>,
    State(state): State<AppState>,
) -> Json<ApiResponse<Vec<Item>>> {
    // 业务逻辑
    let items = get_items(&params).await?;
    ApiResponseBuilder::success(items)
}
```

#### 添加新设备驱动

1. 在 `src/domain/device/driver/drivers/` 创建驱动文件
2. 实现 `DeviceDriver` trait
3. 在 `mod.rs` 中注册驱动

```rust
// 示例：创建新驱动
use crate::domain::device::driver::{DeviceDriver, DriverResult};

pub struct MyCustomDriver {
    // 驱动配置
}

#[async_trait::async_trait]
impl DeviceDriver for MyCustomDriver {
    async fn connect(&mut self) -> DriverResult<()> {
        // 连接逻辑
    }
    
    async fn read_data(&mut self) -> DriverResult<Vec<u8>> {
        // 数据读取逻辑
    }
}
```

### 前端开发

#### 创建新Service

1. 在 `web/service/` 目录创建service文件
2. 使用统一的API客户端
3. 创建对应的React Query hooks

```typescript
// web/service/items.ts
import { apiGet, apiPost } from '@/lib/api-client'
import { useQuery, useMutation } from '@tanstack/react-query'
import { queryKeys } from '@/lib/query-keys'

export interface Item {
  id: string
  name: string
  createdAt: string
}

export const itemApi = {
  getItems: (params?: { page?: number }) => 
    apiGet<Item[]>('items', params),
  createItem: (data: CreateItemRequest) => 
    apiPost<Item>('items', data),
}

export const useItems = (params?: { page?: number }) => {
  return useQuery({
    queryKey: queryKeys.items.list(params || {}),
    queryFn: async () => {
      const response = await itemApi.getItems(params)
      return response.result || []
    },
  })
}
```

#### 创建新组件

1. 在 `web/app/components/` 相应目录创建组件
2. 使用service层提供的hooks
3. 遵循组件命名规范

```typescript
// web/app/components/items/item-list.tsx
import { useItems } from '@/service/items'

const ItemList: React.FC = () => {
  const { data: items, isLoading, error } = useItems({ page: 1 })
  
  if (isLoading) return <div>加载中...</div>
  if (error) return <div>加载失败: {error.message}</div>
  
  return (
    <div>
      {items?.map(item => (
        <div key={item.id}>{item.name}</div>
      ))}
    </div>
  )
}

export default ItemList
```

### 开发工具

#### 代码格式化和检查

```bash
# 后端
cd api
cargo fmt          # 格式化代码
cargo check        # 检查代码
cargo clippy       # 代码检查

# 前端
cd web
pnpm lint          # ESLint检查
pnpm lint:fix      # 自动修复
pnpm format        # Prettier格式化
pnpm type-check    # TypeScript检查
```

#### API测试

```bash
# 验证驱动API
./scripts/verify-driver-api.sh

# API格式检查
python3 scripts/test-api-format.py
```

## 鸿蒙系统部署

### 构建和部署

详细部署指南请参考：
- [鸿蒙部署指南](HARMONYOS_DEPLOYMENT_GUIDE.md)
- [快速开始](QUICK_START_HARMONYOS.md)
- [构建说明](build-harmonyos.md)

使用部署脚本：
```bash
# Linux/macOS
./deploy-to-harmonyos.sh

# Windows
.\build-harmonyos.bat

# 或使用构建脚本
./build-harmonyos.sh
```

## MQTT 主题

```
gateway/{sn}/heartbeat        # 心跳消息
gateway/{sn}/device_regist    # 设备注册
gateway/{sn}/command          # 命令下发
gateway/{sn}/device_command   # 设备命令
gateway/{sn}/data             # 数据上传
gateway/{sn}/alarm            # 告警消息
```

## 项目状态

✅ **最新完成的工作**:
- **API 规范统一**: 建立完整的前后端API开发规范，确保数据对接一致性
- **统一响应格式**: 所有API使用 `ApiResponse<T>` 包装格式
- **前端架构优化**: 统一API客户端，service层规范，React Query集成
- **设备创建向导**: 完整的模板选择和设备配置流程
- **驱动管理系统**: 动态驱动加载，配置参数管理
- **多语言支持**: 模板和界面的国际化处理

✅ **核心功能**:
- **REST API 系统**: 基于 Axum 的现代化 API，统一响应格式
- **前端界面**: Next.js + TypeScript + TailwindCSS 现代化界面
- **设备驱动系统**: 支持重试机制和状态管理的驱动框架
- **设备模板系统**: 模板化设备创建，支持验证和预览
- **配置管理**: 多源配置加载，环境变量覆盖，配置验证
- **认证授权**: JWT 会话管理，角色权限控制
- **监控告警**: 健康检查，指标收集，告警规则
- **鸿蒙系统适配**: 硬件抽象层，资源优化配置

🔧 **技术栈**:
- **后端**: Rust 2021 + Axum + SQLite + Tokio
- **前端**: Next.js 14 + TypeScript + TailwindCSS + React Query
- **数据库**: SQLite + SQLx (自动迁移)
- **认证**: JWT + 会话管理
- **通信协议**: MQTT, HTTP, Modbus RTU/TCP, ONVIF, SNMP
- **包管理**: pnpm (前端)，Cargo (后端)


## 许可证

MIT License - 详见 [license](license) 文件