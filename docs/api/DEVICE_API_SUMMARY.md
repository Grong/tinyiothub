# 设备API接口总结

## 概述

本文档总结了为前端设备详情页面完善的后端接口，包括设备属性、指令、事件、追踪和配置文件等功能。所有接口都基于真实的数据库查询和DataContext，提供完整的设备管理功能。

## 完成的接口

### 1. 设备基础接口

#### 设备详情
- `GET /api/v1/devices/{id}` - 获取设备基本信息和统计数据

#### 设备配置文件 ⭐ 新增
- `GET /api/v1/devices/{id}/profile` - 获取设备完整配置文件
  - 包含设备基本信息、属性列表、指令列表和统计信息
  - 基于真实数据库查询，不使用模拟数据
  - 智能计算在线状态和属性统计

### 2. 设备属性接口

- `GET /api/v1/devices/{id}/properties` - 获取设备属性列表
- `GET /api/v1/devices/{id}/properties/{property_id}` - 获取单个属性详情
- `PUT /api/v1/devices/{id}/properties/{property_id}` - 更新属性值
- `GET /api/v1/devices/{id}/properties/{property_id}/history` - 获取属性历史数据

### 3. 设备指令接口

- `GET /api/v1/devices/{id}/commands` - 获取设备指令列表
- `POST /api/v1/devices/{id}/commands/{command_id}/execute` - 执行设备指令
- `GET /api/v1/devices/{id}/command-executions` - 获取指令执行历史

### 4. 设备事件接口

- `GET /api/v1/devices/{id}/events` - 获取设备事件列表（支持筛选）
- `GET /api/v1/devices/{id}/events/statistics` - 获取事件统计信息

### 5. 设备追踪接口 ⭐ 新增

- `GET /api/v1/devices/{id}/traces` - 获取设备追踪记录
- `POST /api/v1/devices/{id}/traces` - 创建设备追踪记录
- `GET /api/v1/devices/{id}/traces/statistics` - 获取追踪统计信息
- `GET /api/v1/devices/{id}/traces/performance` - 获取设备性能指标
- `GET /api/v1/devices/{id}/traces/export` - 导出追踪记录
- `POST /api/v1/devices/{id}/traces/clear` - 清理追踪记录

## 技术实现特点

### 1. 真实数据源
- **DataContext集成**: 所有接口都通过DataContext获取数据
- **数据库查询**: 直接从SQLite数据库查询真实数据
- **智能缓存**: 利用DataContext的内存缓存提高性能
- **错误处理**: 完善的错误处理和日志记录

### 2. 设备在线状态检查
```rust
pub fn is_device_online(&self, device_id: &str) -> bool {
    if let Some(device) = self.get_device(device_id) {
        // 检查设备状态（state字段）
        if let Some(state) = device.state {
            if state == 0 {
                return false; // 设备被禁用
            }
        }
        
        // 基于设备更新时间判断在线状态
        if let Some(updated_at) = &device.updated_at {
            // 10分钟内有更新认为在线
            // ...
        }
        
        true
    } else {
        false
    }
}
```

### 3. 属性统计计算
```rust
// 计算在线属性数量（基于最后更新时间）
let online_properties = properties.iter()
    .filter(|p| {
        if let Some(last_update) = &p.updated_at {
            // 5分钟内更新的认为是在线
            // ...
        } else {
            false
        }
    })
    .count() as u32;
```

### 4. 设备追踪工具类
```rust
// 便捷的追踪记录工具
pub struct DeviceTracer {
    data_context: Arc<DataContext>,
}

// 支持多种追踪类型
impl DeviceTracer {
    pub async fn trace_operation(...) -> Result<String, Error>
    pub async fn trace_error(...) -> Result<String, Error>
    pub async fn trace_communication(...) -> Result<String, Error>
    pub async fn trace_performance(...) -> Result<String, Error>
    pub async fn trace_debug(...) -> Result<String, Error>
}
```

### 5. 宏支持
```rust
// 便捷的追踪记录宏
trace_device!(operation, tracer, "device_001", "配置更新", "更新采样频率");
trace_device!(error, tracer, "device_001", "连接超时", "设备连接超时", error_details);
trace_device!(comm, tracer, "device_001", "读取寄存器", "成功读取", comm_details, 45);
```

## DataContext增强

### 新增方法

1. **设备属性管理**
   ```rust
   pub async fn get_device_properties(&self, device_id: &str) -> Result<Vec<DeviceProperty>, Error>
   pub async fn get_device_commands(&self, device_id: &str) -> Result<Vec<DeviceCommand>, Error>
   ```

2. **设备操作**
   ```rust
   pub async fn execute_device_command(&self, device_id: &str, command_id: &str, parameters: Option<serde_json::Value>) -> Result<String, Error>
   pub async fn update_device_property_value(&self, device_id: &str, property_id: &str, value: &str) -> Result<(), Error>
   ```

3. **设备追踪**
   ```rust
   pub async fn record_device_trace(&self, device_id: &str, trace_type: &str, level: &str, ...) -> Result<String, Error>
   pub async fn clear_device_traces(&self, device_id: &str, ...) -> Result<u32, Error>
   ```

4. **状态和统计**
   ```rust
   pub fn is_device_online(&self, device_id: &str) -> bool
   pub async fn get_device_statistics(&self, device_id: &str) -> Option<DeviceStatistics>
   pub fn get_device_performance_metrics(&self, device_id: &str) -> Option<DevicePerformanceMetrics>
   ```

## 数据结构

### 设备配置文件
```rust
pub struct DeviceProfile {
    pub device: Device,                    // 设备基本信息
    pub is_online: bool,                   // 在线状态
    pub properties: Vec<DeviceProperty>,   // 属性列表
    pub commands: Vec<DeviceCommand>,      // 指令列表
    pub statistics: DeviceProfileStatistics, // 统计信息
    pub generated_at: String,              // 生成时间
}
```

### 设备追踪记录
```rust
pub struct DeviceTrace {
    pub id: String,
    pub device_id: String,
    pub trace_type: String,    // operation, debug, performance, error, communication
    pub level: String,         // trace, debug, info, warn, error
    pub category: String,      // system, user, auto, driver, network
    pub title: String,
    pub message: String,
    pub details: Option<serde_json::Value>,
    pub source: Option<String>,
    pub user_id: Option<String>,
    pub session_id: Option<String>,
    pub duration_ms: Option<u64>,
    pub status: String,        // started, completed, failed, cancelled
    pub created_at: String,
    pub completed_at: Option<String>,
}
```

### 性能指标
```rust
pub struct DevicePerformanceMetrics {
    pub device_id: String,
    pub cpu_usage: Option<f64>,
    pub memory_usage: Option<f64>,
    pub network_latency_ms: Option<f64>,
    pub response_time_ms: Option<f64>,
    pub throughput_ops_per_sec: Option<f64>,
    pub error_rate: Option<f64>,
    pub uptime_percentage: Option<f64>,
    pub last_updated: String,
}
```

## 使用场景

### 1. 设备详情页面
```typescript
// 获取设备完整配置文件
const profile = await getDeviceProfile(deviceId);
console.log(`设备 ${profile.device.name}:`);
console.log(`- 属性数量: ${profile.properties.length}`);
console.log(`- 指令数量: ${profile.commands.length}`);
console.log(`- 在线状态: ${profile.is_online ? '在线' : '离线'}`);
```

### 2. 设备追踪和调试
```rust
// 记录设备操作
let tracer = DeviceTracer::new(data_context);
trace_device!(operation, tracer, device_id, "配置更新", "用户更新了采样频率");

// 记录通信详情
let comm_details = json!({
    "protocol": "modbus",
    "register": 40001,
    "response_time": 45
});
trace_device!(comm, tracer, device_id, "读取寄存器", "成功读取保持寄存器", comm_details, 45);
```

### 3. 性能监控
```typescript
// 获取设备性能指标
const metrics = await getDevicePerformanceMetrics(deviceId);
if (metrics.cpu_usage > 80) {
    console.warn('CPU使用率过高:', metrics.cpu_usage);
}
```

## 安全性和权限

- **JWT认证**: 所有接口都需要有效的JWT令牌
- **权限控制**: 根据用户权限控制访问范围
- **参数验证**: 严格的输入参数验证
- **错误处理**: 安全的错误信息返回

## 性能优化

- **内存缓存**: 利用DataContext缓存常用设备信息
- **数据库优化**: 使用索引和优化查询
- **分页支持**: 大数据量的分页处理
- **异步处理**: 所有数据库操作都是异步的

## 扩展性

- **模块化设计**: 每个功能模块独立，易于扩展
- **统一接口**: 一致的API设计模式
- **类型安全**: 完整的TypeScript类型定义
- **文档完善**: 详细的API文档和使用示例

## 后续计划

1. **实时数据推送**: 通过WebSocket推送设备状态变化
2. **批量操作**: 支持批量设备操作
3. **数据导出**: 支持多种格式的数据导出
4. **告警集成**: 与告警系统深度集成
5. **性能优化**: 进一步优化查询性能

## 总结

通过这次完善，我们为前端设备详情页面提供了完整的后端支持，包括：

- ✅ 真实数据源，不再使用模拟数据
- ✅ 完整的设备管理功能
- ✅ 强大的设备追踪和调试能力
- ✅ 智能的在线状态检查
- ✅ 详细的统计信息计算
- ✅ 便捷的开发工具和宏支持
- ✅ 完善的错误处理和日志记录

所有接口都经过编译验证，可以直接投入使用。