# 设备监控界面

这个模块提供了完整的设备监控界面，包括设备状态、性能指标、告警信息和追踪记录等功能。

## 组件结构

```
monitoring/
├── index.tsx                           # 导出所有组件
├── device-monitoring-dashboard.tsx     # 主监控面板
├── device-status-card.tsx             # 设备状态卡片
├── device-performance-metrics.tsx     # 性能指标组件
├── device-performance-chart.tsx       # 性能趋势图表
├── device-performance-alerts.tsx      # 性能告警组件
├── device-trace-overview.tsx          # 追踪记录概览
├── device-trace-records.tsx           # 追踪记录详情
├── example-usage.tsx                  # 使用示例
└── README.md                          # 文档
```

## 主要功能

### 1. 设备状态监控
- 实时在线/离线状态
- 连接质量评分
- 设备基本统计信息（属性、指令、事件、告警数量）

### 2. 性能指标监控
- CPU 使用率
- 内存使用率  
- 网络延迟
- 响应时间
- 吞吐量
- 错误率
- 正常运行时间

### 3. 性能趋势分析
- 历史性能数据图表
- 可配置时间范围（1小时-7天）
- 多指标趋势对比

### 4. 性能告警
- 实时性能告警
- 分级告警（警告/严重）
- 告警详情和阈值显示

### 5. 追踪记录
- 设备操作日志
- 多维度过滤（类型、级别）
- 详细记录查看
- 记录清理功能

## 使用方法

### 基本使用

```tsx
import { DeviceMonitoringDashboard } from '@/app/components/devices/monitoring'

const DeviceDetailPage = ({ deviceId }: { deviceId: string }) => {
  return (
    <DeviceMonitoringDashboard 
      deviceId={deviceId}
      deviceName="设备名称"
    />
  )
}
```

### 单独使用组件

```tsx
import { 
  DeviceStatusCard,
  DevicePerformanceMetrics,
  DevicePerformanceAlerts 
} from '@/app/components/devices/monitoring'

const CustomMonitoringPage = ({ deviceId }: { deviceId: string }) => {
  const { data: deviceStatus } = useDeviceStatus(deviceId)
  const { data: performanceMetrics } = useDevicePerformance(deviceId)
  const { data: alerts } = useDevicePerformanceAlerts(deviceId)

  return (
    <div className="space-y-6">
      <DeviceStatusCard deviceStatus={deviceStatus} />
      <DevicePerformanceMetrics performanceMetrics={performanceMetrics} />
      <DevicePerformanceAlerts alerts={alerts} deviceId={deviceId} />
    </div>
  )
}
```

## API 依赖

监控界面依赖以下后端 API 端点：

### 设备状态相关
- `GET /api/v1/devices/:device_id/status` - 获取设备在线状态
- `GET /api/v1/devices/:device_id/statistics` - 获取设备统计信息

### 性能监控相关  
- `GET /api/v1/devices/:device_id/performance` - 获取设备性能指标
- `GET /api/v1/devices/:device_id/performance/history?hours=24` - 获取性能历史数据
- `GET /api/v1/devices/:device_id/performance/alerts` - 获取性能告警

### 追踪记录相关
- `GET /api/v1/devices/:device_id/traces` - 获取追踪记录
- `POST /api/v1/devices/:device_id/traces` - 创建追踪记录
- `GET /api/v1/devices/:device_id/traces/statistics` - 获取追踪统计
- `POST /api/v1/devices/:device_id/traces/clear` - 清理追踪记录

所有 API 都应该返回统一的 `ApiResponse<T>` 格式：

```typescript
{
  code: 0,           // 0表示成功，非0表示错误
  msg: "",           // 错误信息，成功时为空字符串  
  result: T | null   // 实际数据，错误时为null
}
```

## 数据类型

### DeviceOnlineStatus
```typescript
interface DeviceOnlineStatus {
  deviceId: string
  isOnline: boolean
  connectionQuality?: number  // 0-100
  lastCheck: string
}
```

### DeviceStatistics
```typescript
interface DeviceStatistics {
  totalProperties: number
  onlineProperties: number
  offlineProperties: number
  totalCommands: number
  totalEvents: number
  activeAlarms: number
}
```

### DevicePerformanceMetrics
```typescript
interface DevicePerformanceMetrics {
  deviceId: string
  cpuUsage?: number
  memoryUsage?: number
  networkLatencyMs?: number
  responseTimeMs?: number
  throughputOpsPerSec?: number
  errorRate?: number
  uptimePercentage?: number
  lastUpdated: string
}
```

### PerformanceAlert
```typescript
interface PerformanceAlert {
  deviceId: string
  alertType: string    // high_cpu, high_memory, high_latency, etc.
  severity: string     // warning, critical
  message: string
  currentValue: number
  threshold: number
  timestamp: string
}
```

### DeviceTrace
```typescript
interface DeviceTrace {
  id: string
  deviceId: string
  traceType: string    // operation, status_change, error, warning, info
  level: string        // debug, info, warn, error, critical
  category: string     // system, user, device, network, performance
  title: string
  message: string
  details?: string     // JSON 格式的详细信息
  source?: string      // api, system, device, scheduler
  userId?: string
  sessionId?: string
  createdAt: string
}
```

## 样式和主题

组件使用项目统一的设计系统：

- 颜色：使用 `text-*` 和 `components-*` CSS 变量
- 间距：使用 Tailwind CSS 间距系统
- 组件：基于项目现有的设计模式
- 响应式：支持移动端和桌面端

## 性能优化

- 使用 React Query 进行数据缓存和自动刷新
- 组件懒加载和代码分割
- 合理的刷新间隔设置
- 分页和虚拟滚动支持

## 扩展性

组件设计考虑了扩展性：

- 模块化设计，可单独使用
- 支持自定义样式和主题
- 可配置的刷新间隔和数据范围
- 插件化的图表和可视化支持

## 故障排除

### 常见问题

1. **数据不显示**
   - 检查 API 端点是否正确
   - 确认后端返回 `ApiResponse<T>` 格式
   - 检查设备 ID 是否有效

2. **性能图表为空**
   - 确认设备有性能历史数据
   - 检查时间范围设置
   - 验证 API 返回的数据格式

3. **告警不更新**
   - 检查自动刷新是否启用
   - 确认后端告警计算逻辑
   - 验证告警阈值设置

4. **追踪记录加载慢**
   - 调整分页大小
   - 使用过滤器减少数据量
   - 检查数据库索引优化

### 调试技巧

- 使用浏览器开发者工具查看网络请求
- 检查 React Query DevTools
- 查看控制台错误信息
- 使用 React DevTools 检查组件状态

## 更新日志

### v1.0.0 (2026-01-10)
- 初始版本发布
- 完整的设备监控界面
- 支持状态、性能、告警、追踪记录
- 响应式设计和移动端支持