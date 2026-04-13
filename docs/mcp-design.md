> ⚠️ **已弃用**：本文档描述的独立 `mcp/` crate 架构已删除。MCP Server 现已内嵌到 `api/src/api/mcp/` 中。本文件仅保留供历史参考。

# TinyIoTHub MCP 协议支持 - 技术方案设计

## 一、架构设计

### 1.1 整体架构

```
┌─────────────────────────────────────────────────────────────────────┐
│                         AI Client                                    │
│              (Claude Desktop, Cursor, etc.)                          │
└─────────────────────────┬───────────────────────────────────────────┘
                          │ STDIO / HTTP
                          ▼
┌─────────────────────────────────────────────────────────────────────┐
│                     MCP Server (tinyiothub-mcp)                      │
│  ┌──────────────┐  ┌──────────────┐  ┌──────────────┐              │
│  │   Server     │  │    Tools     │  │   Resources  │              │
│  │   Core       │  │   Handler    │  │   Provider   │              │
│  └──────────────┘  └──────────────┘  └──────────────┘              │
└─────────────────────────┬───────────────────────────────────────────┘
                          │
                          ▼
┌─────────────────────────────────────────────────────────────────────┐
│                    TinyIoTHub Core                                   │
│  ┌──────────────┐  ┌──────────────┐  ┌──────────────┐              │
│  │  REST API    │  │  Application │  │   Domain     │              │
│  │   Layer      │  │    Layer     │  │    Layer     │              │
│  └──────────────┘  └──────────────┘  └──────────────┘              │
└─────────────────────────┬───────────────────────────────────────────┘
                          │
                          ▼
┌─────────────────────────────────────────────────────────────────────┐
│                    Infrastructure                                    │
│  ┌──────────────┐  ┌──────────────┐  ┌──────────────┐              │
│  │  Database    │  │    MQTT      │  │   Drivers    │              │
│  │  (SQLite)   │  │   Broker     │  │  (Modbus..)  │              │
│  └──────────────┘  └──────────────┘  └──────────────┘              │
└─────────────────────────────────────────────────────────────────────┘
```

### 1.2 两种集成方案

#### 方案 A：独立进程（推荐）

```
┌─────────────┐     ┌─────────────┐
│  tinyiothub │     │ tinyiothub  │
│    (API)    │◀───▶│    -mcp     │
│  :3002      │ HTTP│  (STDIO)    │
└─────────────┘     └─────────────┘
```

- **优点**：解耦、独立演进、部署灵活
- **缺点**：需要额外进程管理
- **适用**：生产环境

#### 方案 B：进程内集成

```
┌─────────────────────────────────────────────┐
│              tinyiothub                      │
│  ┌─────────────┐   ┌─────────────┐         │
│  │  HTTP API   │   │ MCP Server  │         │
│  │  :3002      │   │  STDIO     │         │
│  └─────────────┘   └─────────────┘         │
└─────────────────────────────────────────────┘
```

- **优点**：单一进程、简单
- **缺点**：耦合、影响主服务
- **适用**：开发/测试、快速原型

**决策**：推荐方案 A，独立进程

### 1.3 通信协议

| 传输方式 | 说明 | 适用场景 |
|----------|------|----------|
| **STDIO** | 标准输入输出 | 本地 AI 客户端 |
| **HTTP + SSE** | HTTP 长连接 | 远程部署、Claude Code |

## 二、MCP Server 模块设计

### 2.1 项目结构

```
tinyiothub/
├── mcp/                              # MCP Server (独立 crate)
│   ├── src/
│   │   ├── main.rs                   # 入口
│   │   ├── server.rs                 # MCP 服务器实现
│   │   ├── tools/                    # 工具定义
│   │   │   ├── mod.rs
│   │   │   ├── device.rs             # 设备相关工具
│   │   │   ├── alarm.rs              # 告警相关工具
│   │   │   └── data.rs               # 数据查询工具
│   │   ├── resources/                # 资源定义
│   │   │   ├── mod.rs
│   │   │   └── device_resource.rs
│   │   ├── transport/                # 传输层
│   │   │   ├── mod.rs
│   │   │   ├── stdio.rs
│   │   │   └── http.rs
│   │   └── client/                   # TinyIoTHub API 客户端
│   │       ├── mod.rs
│   │       └── api_client.rs
│   ├── Cargo.toml
│   └── mcp_settings.toml            # MCP 配置
```

### 2.2 核心模块

```rust
// mcp/src/server.rs
pub struct McpServer {
    client: TinyIoTHubClient,
    tools: Vec<Tool>,
    resources: Vec<Resource>,
}

impl McpServer {
    pub fn new(config: McpConfig) -> Self {
        let client = TinyIoTHubClient::new(&config.api_url, &config.api_key);
        
        Self {
            client,
            tools: Self::build_tools(),
            resources: Self::build_resources(),
        }
    }
    
    fn build_tools() -> Vec<Tool> {
        vec![
            // 设备管理
            tool!("list_devices", "列出所有设备"),
            tool!("get_device", "获取设备详情"),
            tool!("get_device_status", "获取设备状态"),
            // 实时控制
            tool!("read_sensor_data", "读取传感器数据"),
            tool!("send_command", "发送控制命令"),
            // 告警管理
            tool!("list_alarms", "列出告警"),
            tool!("acknowledge_alarm", "确认告警"),
            // 数据查询
            tool!("query_device_history", "查询历史数据"),
        ]
    }
}
```

## 三、工具定义（Tools）

### 3.1 设备管理工具

#### 3.1.1 list_devices

```json
{
  "name": "list_devices",
  "description": "列出所有 IoT 设备，支持分页和过滤",
  "inputSchema": {
    "type": "object",
    "properties": {
      "page": { "type": "integer", "default": 1 },
      "pageSize": { "type": "integer", "default": 20 },
      "status": { "type": "string", "enum": ["online", "offline", "all"], "default": "all" },
      "driver": { "type": "string", "description": "按驱动名称过滤" }
    }
  }
}
```

#### 3.1.2 get_device

```json
{
  "name": "get_device",
  "description": "获取单个设备的详细信息",
  "inputSchema": {
    "type": "object",
    "properties": {
      "device_id": { "type": "string", "description": "设备唯一标识" }
    },
    "required": ["device_id"]
  }
}
```

#### 3.1.3 get_device_status

```json
{
  "name": "get_device_status",
  "description": "获取设备的实时状态（在线/离线）",
  "inputSchema": {
    "type": "object",
    "properties": {
      "device_id": { "type": "string" }
    },
    "required": ["device_id"]
  }
}
```

### 3.2 实时控制工具

#### 3.2.1 read_sensor_data

```json
{
  "name": "read_sensor_data",
  "description": "读取传感器的实时数据",
  "inputSchema": {
    "type": "object",
    "properties": {
      "device_id": { "type": "string" },
      "properties": { 
        "type": "array", 
        "items": { "type": "string" },
        "description": "要读取的属性列表，如 [\"temperature\", \"humidity\"]"
      }
    },
    "required": ["device_id"]
  }
}
```

#### 3.2.2 send_command

```json
{
  "name": "send_command",
  "description": "向设备发送控制命令",
  "inputSchema": {
    "type": "object",
    "properties": {
      "device_id": { "type": "string" },
      "command": { "type": "string", "description": "命令名称" },
      "parameters": { 
        "type": "object", 
        "description": "命令参数" 
      }
    },
    "required": ["device_id", "command"]
  }
}
```

### 3.3 告警管理工具

#### 3.3.1 list_alarms

```json
{
  "name": "list_alarms",
  "description": "列出告警事件",
  "inputSchema": {
    "type": "object",
    "properties": {
      "status": { 
        "type": "string", 
        "enum": ["active", "acknowledged", "all"],
        "default": "active" 
      },
      "limit": { "type": "integer", "default": 20 }
    }
  }
}
```

#### 3.3.2 acknowledge_alarm

```json
{
  "name": "acknowledge_alarm",
  "description": "确认告警",
  "inputSchema": {
    "type": "object",
    "properties": {
      "alarm_id": { "type": "string" },
      "comment": { "type": "string", "description": "处理备注" }
    },
    "required": ["alarm_id"]
  }
}
```

### 3.4 数据查询工具

#### 3.4.1 query_device_history

```json
{
  "name": "query_device_history",
  "description": "查询设备历史数据",
  "inputSchema": {
    "type": "object",
    "properties": {
      "device_id": { "type": "string" },
      "property": { "type": "string", "description": "属性名称" },
      "start_time": { "type": "string", "description": "ISO8601 时间" },
      "end_time": { "type": "string", "description": "ISO8601 时间" },
      "limit": { "type": "integer", "default": 100 }
    },
    "required": ["device_id", "start_time", "end_time"]
  }
}
```

## 四、Resources 定义

### 4.1 设备列表资源

```json
{
  "uri": "device://list",
  "name": "设备列表",
  "description": "所有设备的简要信息列表",
  "mimeType": "application/json"
}
```

### 4.2 设备详情资源

```json
{
  "uri": "device://{id}",
  "name": "设备详情",
  "description": "单个设备的完整信息",
  "mimeType": "application/json"
}
```

### 4.3 系统状态资源

```json
{
  "uri": "system://status",
  "name": "系统状态",
  "description": "网关系统运行状态",
  "mimeType": "application/json"
}
```

## 五、配置设计

### 5.1 MCP 配置文件

```toml
# mcp_settings.toml

[mcp.server]
name = "tinyiothub"
version = "1.0.0"

[mcp.transport]
# stdio | http
mode = "stdio"

[mcp.tinyiothub]
# TinyIoTHub API 地址
api_url = "http://localhost:3002"
# API 认证密钥
api_key = "${TINYIOTHUB_API_KEY}"

[mcp.security]
# 是否启用认证
enabled = true
# 允许的工具列表（空 = 全部允许）
allowed_tools = []

[mcp.tools]
# 工具超时时间（秒）
timeout = 30

[mcp.resources]
# 资源缓存时间（秒）
cache_ttl = 60
```

### 5.2 环境变量

```bash
# 必填
TINYIOTHUB_API_URL=http://localhost:3002
TINYIOTHUB_API_KEY=your-api-key-here

# 可选
MCP_TRANSPORT=stdio  # stdio | http
MCP_HTTP_PORT=3003
```

### 5.3 AI 客户端配置

#### Claude Desktop (macOS)

```json
{
  "mcpServers": {
    "tinyiothub": {
      "command": "cargo",
      "args": ["run", "--manifest-path", "/path/to/tinyiothub/mcp/Cargo.toml"]
    }
  }
}
```

#### Claude Desktop (Windows)

```json
{
  "mcpServers": {
    "tinyiothub": {
      "command": "cargo",
      "args": ["run", "--manifest-path", "C:\\path\\to\\tinyiothub\\mcp\\Cargo.toml"]
    }
  }
}
```

## 六、数据流设计

### 6.1 工具调用流程

```
AI Client
    │
    │ 1. "关闭客厅灯"
    ▼
MCP Server
    │
    │ 2. 解析工具调用: send_command
    ▼
TinyIoTHub API Client
    │
    │ 3. HTTP POST /api/v1/devices/{id}/command
    ▼
TinyIoTHub Core
    │
    │ 4. 执行设备驱动
    ▼
IoT Device (Modbus/ONVIF)
    │
    │ 5. 命令执行结果
    ▼
TinyIoTHub Core → API Client → MCP Server → AI Client
```

### 6.2 错误处理

| 错误码 | 说明 | 处理策略 |
|--------|------|----------|
| 400 | 参数错误 | 返回错误信息，指导用户修正 |
| 401 | 未认证 | 提示配置 API Key |
| 404 | 设备不存在 | 提示检查设备 ID |
| 500 | 服务错误 | 返回错误，保留日志 |
| timeout | 超时 | 重试一次，提示用户 |

## 七、认证设计

### 7.1 认证流程

```
AI Client
    │
    │ MCP Handshake
    ▼
MCP Server
    │
    │ 检查 API Key
    ▼
TinyIoTHub API
    │
    │ JWT Token 验证
    ▼
返回认证结果
```

### 7.2 权限控制

| 工具 | 需要的权限 |
|------|-----------|
| list_devices | read:devices |
| get_device | read:devices |
| read_sensor_data | read:devices |
| send_command | write:devices |
| list_alarms | read:alarms |
| acknowledge_alarm | write:alarms |

## 八、实施计划

### 8.1 阶段一：基础框架（Week 1-2）

- [ ] 创建 `mcp` crate
- [ ] 实现 MCP Server 骨架
- [ ] 实现 STDIO 传输
- [ ] 实现 TinyIoTHub API Client
- [ ] 基础配置加载

### 8.2 阶段二：设备工具（Week 3）

- [ ] 实现 list_devices
- [ ] 实现 get_device
- [ ] 实现 get_device_status
- [ ] 实现 read_sensor_data

### 8.3 阶段三：控制工具（Week 4）

- [ ] 实现 send_command
- [ ] 错误处理优化
- [ ] 日志完善

### 8.4 阶段四：扩展功能（Week 5-6）

- [ ] 告警管理工具
- [ ] 历史数据查询
- [ ] HTTP 传输支持
- [ ] 资源（Resources）实现

### 8.5 阶段五：测试与文档（Week 7）

- [ ] 集成测试
- [ ] Claude Desktop 实测
- [ ] 文档完善

## 九、技术风险与缓解

| 风险 | 影响 | 缓解措施 |
|------|------|----------|
| Rust MCP SDK 不成熟 | 开发效率低 | 参考 Python/TS SDK 协议实现 |
| 实时性要求 | 控制延迟 | 优化网络层，缓存策略 |
| 安全性 | 未授权访问 | 强制 API Key + 权限控制 |
| 依赖主服务 | 可用性降低 | 独立进程，监控告警 |

## 十、验收标准

### 10.1 功能验收

- [ ] 能通过 Claude Desktop 连接
- [ ] list_devices 返回正确设备列表
- [ ] read_sensor_data 返回实时数据
- [ ] send_command 能控制设备
- [ ] 错误信息清晰易懂

### 10.2 性能验收

- [ ] 工具响应时间 < 1s
- [ ] 支持 100+ 设备列表
- [ ] 内存占用 < 50MB

### 10.3 稳定性验收

- [ ] 7x24 小时运行稳定
- [ ] 网络异常自动重连
- [ ] 日志完整可追溯

---

*设计方案完成日期：2026-03-15*
