# 设备Trace接口文档

## 概述

设备Trace接口提供了设备操作轨迹记录、调试信息、性能监控等功能。通过这些接口可以追踪设备的所有操作历史，进行故障诊断和性能分析。

## 接口列表

### 1. 获取设备追踪记录

- **路径**: `GET /api/v1/devices/{device_id}/traces`
- **认证**: 需要JWT认证
- **权限**: 需要设备查看权限

#### 查询参数

| 参数名 | 类型 | 必填 | 描述 |
|--------|------|------|------|
| trace_type | string | 否 | 追踪类型：operation, debug, performance, error, communication |
| level | string | 否 | 日志级别：trace, debug, info, warn, error |
| category | string | 否 | 分类：system, user, auto, driver, network |
| status | string | 否 | 状态：started, completed, failed, cancelled |
| start_date | string | 否 | 开始日期 (YYYY-MM-DD HH:MM:SS) |
| end_date | string | 否 | 结束日期 (YYYY-MM-DD HH:MM:SS) |
| search | string | 否 | 搜索关键词 |
| user_id | string | 否 | 用户ID |
| session_id | string | 否 | 会话ID |
| limit | number | 否 | 返回记录数限制 (默认50) |
| offset | number | 否 | 偏移量 (默认0) |

#### 响应示例

```json
{
  "success": true,
  "result": [
    {
      "id": "trace_001",
      "device_id": "device_001",
      "trace_type": "operation",
      "level": "info",
      "category": "user",
      "title": "设备配置更新",
      "message": "用户更新了设备的采样频率配置",
      "details": {
        "old_value": "5s",
        "new_value": "3s",
        "config_key": "sampling_interval"
      },
      "source": "web_ui",
      "user_id": "user_001",
      "session_id": "session_123",
      "duration_ms": 156,
      "status": "completed",
      "created_at": "2024-01-07 16:30:00",
      "completed_at": "2024-01-07 16:30:00"
    }
  ]
}
```

### 2. 创建设备追踪记录

- **路径**: `POST /api/v1/devices/{device_id}/traces`
- **认证**: 需要JWT认证
- **权限**: 需要设备操作权限

#### 请求体

```json
{
  "trace_type": "operation",
  "level": "info",
  "category": "user",
  "title": "手动数据采集",
  "message": "用户手动触发了数据采集操作",
  "details": {
    "trigger_source": "manual",
    "data_points": 25
  },
  "source": "web_ui",
  "session_id": "session_456",
  "duration_ms": 1250
}
```

#### 响应示例

```json
{
  "success": true,
  "result": {
    "id": "trace_002",
    "device_id": "device_001",
    "trace_type": "operation",
    "level": "info",
    "category": "user",
    "title": "手动数据采集",
    "message": "用户手动触发了数据采集操作",
    "details": {
      "trigger_source": "manual",
      "data_points": 25
    },
    "source": "web_ui",
    "user_id": "current_user",
    "session_id": "session_456",
    "duration_ms": 1250,
    "status": "completed",
    "created_at": "2024-01-07 16:35:00",
    "completed_at": "2024-01-07 16:35:00"
  }
}
```

### 3. 获取追踪统计信息

- **路径**: `GET /api/v1/devices/{device_id}/traces/statistics`
- **认证**: 需要JWT认证

#### 响应示例

```json
{
  "success": true,
  "result": {
    "total_traces": 105,
    "traces_by_type": {
      "operation": 45,
      "debug": 23,
      "performance": 18,
      "error": 7,
      "communication": 12
    },
    "traces_by_level": {
      "trace": 15,
      "debug": 28,
      "info": 52,
      "warn": 8,
      "error": 2
    },
    "traces_by_category": {
      "system": 35,
      "user": 28,
      "auto": 22,
      "driver": 15,
      "network": 5
    },
    "recent_errors": 2,
    "average_duration_ms": 245.6,
    "last_trace_time": "2024-01-07 16:30:00"
  }
}
```

### 4. 获取设备性能指标

- **路径**: `GET /api/v1/devices/{device_id}/traces/performance`
- **认证**: 需要JWT认证

#### 响应示例

```json
{
  "success": true,
  "result": {
    "device_id": "device_001",
    "cpu_usage": 45.2,
    "memory_usage": 68.7,
    "network_latency_ms": 12.5,
    "response_time_ms": 156.3,
    "throughput_ops_per_sec": 234.8,
    "error_rate": 0.02,
    "uptime_percentage": 99.8,
    "last_updated": "2024-01-07 16:30:00"
  }
}
```

### 5. 导出追踪记录

- **路径**: `GET /api/v1/devices/{device_id}/traces/export`
- **认证**: 需要JWT认证
- **查询参数**: 支持与获取追踪记录相同的筛选参数

#### 响应示例

```json
{
  "success": true,
  "result": "/api/v1/devices/device_001/traces/download/export_uuid_123"
}
```

### 6. 清理追踪记录

- **路径**: `POST /api/v1/devices/{device_id}/traces/clear`
- **认证**: 需要JWT认证
- **权限**: 需要设备管理权限

#### 响应示例

```json
{
  "success": true,
  "result": 42
}
```

## 数据结构说明

### DeviceTrace

| 字段名 | 类型 | 描述 |
|--------|------|------|
| id | string | 追踪记录ID |
| device_id | string | 设备ID |
| trace_type | string | 追踪类型 |
| level | string | 日志级别 |
| category | string | 分类 |
| title | string | 标题 |
| message | string | 详细消息 |
| details | object? | 详细数据 |
| source | string? | 来源 |
| user_id | string? | 用户ID |
| session_id | string? | 会话ID |
| duration_ms | number? | 持续时间(毫秒) |
| status | string | 状态 |
| created_at | string | 创建时间 |
| completed_at | string? | 完成时间 |

### 追踪类型 (trace_type)

- **operation**: 操作记录 - 用户或系统执行的操作
- **debug**: 调试信息 - 用于故障诊断的详细信息
- **performance**: 性能监控 - 性能指标和监控数据
- **error**: 错误记录 - 错误和异常信息
- **communication**: 通信记录 - 设备通信协议的交互记录

### 日志级别 (level)

- **trace**: 最详细的调试信息
- **debug**: 调试信息
- **info**: 一般信息
- **warn**: 警告信息
- **error**: 错误信息

### 分类 (category)

- **system**: 系统自动生成
- **user**: 用户操作触发
- **auto**: 自动任务触发
- **driver**: 设备驱动生成
- **network**: 网络相关

## 使用场景

### 1. 故障诊断
```typescript
// 获取设备的错误记录
const errorTraces = await getDeviceTraces(deviceId, {
  trace_type: 'error',
  level: 'error',
  limit: 20
});

// 分析错误模式
console.log('最近错误:', errorTraces);
```

### 2. 性能监控
```typescript
// 获取性能指标
const metrics = await getDevicePerformanceMetrics(deviceId);

// 监控CPU使用率
if (metrics.cpu_usage > 80) {
  console.warn('CPU使用率过高:', metrics.cpu_usage);
}
```

### 3. 操作审计
```typescript
// 获取用户操作记录
const userOperations = await getDeviceTraces(deviceId, {
  trace_type: 'operation',
  category: 'user',
  user_id: 'user_001'
});

// 审计用户操作
console.log('用户操作历史:', userOperations);
```

### 4. 调试分析
```typescript
// 记录调试信息
await createDeviceTrace(deviceId, {
  trace_type: 'debug',
  level: 'debug',
  category: 'driver',
  title: 'Modbus通信调试',
  message: '读取寄存器操作详情',
  details: {
    register_address: 40001,
    register_count: 10,
    response_data: [1, 2, 3, 4, 5]
  }
});
```

## 最佳实践

### 1. 追踪记录管理
- 定期清理旧的追踪记录，避免数据库过大
- 根据重要性设置不同的保留期限
- 对敏感信息进行脱敏处理

### 2. 性能考虑
- 避免过于频繁的追踪记录创建
- 使用异步方式记录追踪信息
- 合理设置查询的limit参数

### 3. 安全性
- 确保敏感操作的追踪记录完整性
- 对追踪记录进行访问权限控制
- 防止追踪记录被恶意篡改

## 错误码说明

| 错误码 | 描述 | 解决方案 |
|--------|------|----------|
| DEVICE_NOT_FOUND | 设备不存在 | 检查设备ID是否正确 |
| PERMISSION_DENIED | 权限不足 | 确认用户具有相应权限 |
| INVALID_TRACE_TYPE | 无效的追踪类型 | 使用支持的追踪类型 |
| TRACE_LIMIT_EXCEEDED | 追踪记录数量超限 | 减少查询范围或增加筛选条件 |

## 更新日志

- **v1.0.0** (2024-01-07): 初始版本，支持基本的设备追踪功能