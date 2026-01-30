# Dashboard 页面实现文档

## 概述

基于 IoT Edge 网关项目需求，实现了一个功能完整的 Dashboard 页面，提供系统概览、实时监控、告警管理等核心功能。

## 功能模块

### 1. 统计卡片区域 (StatsCards)
- **设备总数**: 显示系统中的设备总数和月增长量
- **在线设备**: 显示在线设备数量和在线率
- **活跃告警**: 显示当前需要处理的告警数量
- **系统状态**: 显示系统运行状态和运行时间

### 2. 设备状态分布图 (DeviceStatusChart)
- 饼图显示设备状态分布（在线/离线/故障/维护）
- 状态列表显示具体数量和百分比
- 支持空状态显示

### 3. 最新告警列表 (RecentAlarms)
- 显示最近的告警信息
- 按告警级别分类（严重/错误/警告/信息）
- 显示告警状态（活跃/已确认/已解决）
- 智能时间显示（刚刚/分钟前/小时前）

### 4. 系统性能监控 (SystemMetrics)
- CPU 使用率进度条
- 内存使用率进度条
- 磁盘使用率进度条
- 网络流量统计（上行/下行）

### 5. 关键设备快速访问 (QuickDevices)
- 显示重要设备的状态
- 快速跳转到设备详情
- 显示设备类型和最后在线时间

## 技术实现

### 文件结构
```
web/
├── app/components/dashboard/
│   ├── stats-cards.tsx          # 统计卡片组件
│   ├── device-status-chart.tsx  # 设备状态分布图
│   ├── recent-alarms.tsx        # 最新告警列表
│   ├── system-metrics.tsx       # 系统性能监控
│   └── quick-devices.tsx        # 关键设备列表
├── app/(commonLayout)/dashboard/
│   └── page.tsx                 # Dashboard 主页面
├── service/
│   └── dashboard.ts             # Dashboard 数据服务
├── types/
│   └── dashboard.ts             # Dashboard 类型定义
└── lib/
    └── query-keys.ts            # 查询键定义（已更新）
```

### 类型定义
- `DashboardStats`: 系统统计信息
- `DeviceStatusDistribution`: 设备状态分布
- `RecentAlarm`: 告警信息
- `DashboardMetrics`: 系统性能指标（重命名避免冲突）
- `QuickDevice`: 快速访问设备信息

### API 接口设计
```typescript
// 需要后端实现的 API 端点
GET /api/monitoring/stats           # 获取系统统计
GET /api/devices/distribution       # 获取设备状态分布
GET /api/alarms/recent             # 获取最新告警
GET /api/monitoring/metrics        # 获取系统性能指标
GET /api/devices/quick             # 获取关键设备列表
```

### 数据刷新策略
- **统计数据**: 30秒缓存，1分钟自动刷新
- **告警数据**: 15秒缓存，30秒自动刷新
- **性能指标**: 10秒缓存，30秒自动刷新
- **设备数据**: 30秒缓存，1分钟自动刷新

## 响应式设计

### 布局适配
- **桌面端**: 3列布局（左侧2列 + 右侧1列）
- **平板端**: 2列布局
- **移动端**: 单列布局

### 组件适配
- 统计卡片：4列 → 2列 → 1列
- 图表组件：自适应容器宽度
- 列表组件：响应式间距和字体大小

## 性能优化

### 数据加载
- 使用 TanStack Query 进行数据缓存和状态管理
- 智能刷新策略，避免不必要的 API 调用
- 骨架屏加载状态，提升用户体验

### 组件优化
- 使用 React.memo 避免不必要的重渲染
- 合理的组件拆分，提高代码可维护性
- 类型安全的 TypeScript 实现

## 扩展性

### 图表集成
当前使用简化的图表显示，后续可以集成：
- Chart.js
- ECharts
- Recharts

### 实时更新
可以通过以下方式实现实时数据更新：
- WebSocket 连接
- Server-Sent Events (SSE)
- 轮询优化

### 自定义配置
支持用户自定义 Dashboard 布局：
- 拖拽排序
- 显示/隐藏组件
- 个性化设置

## 使用说明

1. **页面访问**: 登录后默认跳转到 Dashboard 页面
2. **数据刷新**: 页面会自动刷新数据，也可手动刷新
3. **快速导航**: 点击相关区域可快速跳转到对应功能页面
4. **响应式**: 支持各种屏幕尺寸的设备访问

## 注意事项

1. **类型冲突**: Dashboard 中的 `SystemMetrics` 重命名为 `DashboardMetrics` 避免与系统模块冲突
2. **API 依赖**: 需要后端实现对应的 API 接口才能显示真实数据
3. **权限控制**: 部分功能可能需要根据用户权限进行显示控制
4. **错误处理**: 已实现基本的错误状态显示，可根据需要扩展

## 后续计划

1. **图表增强**: 集成专业图表库，提供更丰富的数据可视化
2. **实时监控**: 实现 WebSocket 连接，提供实时数据更新
3. **告警推送**: 集成浏览器通知，及时推送重要告警
4. **数据导出**: 支持 Dashboard 数据的导出功能
5. **主题定制**: 支持深色模式和主题定制