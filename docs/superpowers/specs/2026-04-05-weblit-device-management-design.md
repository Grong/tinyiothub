# web-lit 设备管理功能设计文档

> 状态: 已批准
> 日期: 2026-04-05

## 概述

为 web-lit 实现完整的设备管理功能，包括：设备列表增强、新建设备向导、设备详情页增强、监控功能。

## 用户决策

| 决策点 | 选择 |
|-------|------|
| 新建设备向导布局 | 全屏弹窗，两步式 |
| 设备列表视图 | 表格+网格双视图，默认网格 |
| 性能图表渲染 | uPlot 轻量图表库 |
| 监控功能入口 | 设备详情内嵌 Tab |
| 属性曲线刷新 | 10 秒自动轮询 |
| 驱动配置验证 | 实时验证 |

---

## Phase 1: 设备列表页增强

### 1.1 状态 Tab 补充

在 `devices-page.ts` 中添加"维护"状态选项：

```typescript
const statusOptions = [
  { value: '', text: '全部' },
  { value: 'online', text: '在线' },
  { value: 'offline', text: '离线' },
  { value: 'error', text: '错误' },
  { value: 'maintenance', text: '维护' },
]
```

### 1.2 设备卡片网格

**新建**: `web-lit/src/components/device-card.ts`

```typescript
@customElement('device-card')
export class DeviceCard extends LitElement {
  @property({ type: Object }) device!: Device
  @property({ type: Function }) onEdit!: (d: Device) => void
  @property({ type: Function }) onDelete!: (d: Device) => void
  @property({ type: Function }) onRefresh!: () => void
}
```

**布局**: 响应式网格
- `>= 5xl`: 6 列
- `>= 4xl`: 5 列
- `>= 3xl`: 4 列
- `>= 2xl`: 3 列
- `< 2xl`: 2 列

**卡片内容**:
- 状态指示灯 (左下角)
- 设备名称 (顶部)
- 协议类型 (中部)
- 地址 (底部，单线省略)
- 操作按钮 (编辑/删除)

### 1.3 标签筛选

**新建**: `web-lit/src/services/tags.ts`
**新建**: `web-lit/src/components/tag-filter.ts`

```typescript
// API
GET /api/v1/tags?type=device

// 接口
interface Tag {
  id: string
  name: string
  color: string
}

// 组件: 多选下拉，显示已选标签
```

### 1.4 "仅显示我创建的"筛选

**修改**: `devices-page.ts`

添加 Checkbox，查询参数传递 `isCreatedByMe: true/false`

### 1.5 视图切换

**修改**: `devices-page.ts`

- 工具栏添加视图切换按钮 (表格图标 / 网格图标)
- 用户偏好保存 `localStorage.setItem('deviceListView', 'grid' | 'table')`
- 默认视图: 网格

---

## Phase 2: 新建设备向导

### 2.1 全屏弹窗结构

**新建**: `web-lit/src/components/create-device-wizard.ts`

```
FullScreenModal
├── Header
│   ├── 标题: "创建设备"
│   └── StepIndicator (步骤指示器)
├── Body
│   └── StepContent
│       ├── Step 1: TemplateSelectionStep
│       │   ├── 搜索框
│       │   └── 分类 Tab + TemplateCard 网格
│       └── Step 2: DeviceInfoStep
│           ├── 左侧表单
│           │   ├── 模板摘要
│           │   ├── 设备名称* (必填)
│           │   ├── 设备描述
│           │   ├── 设备地址
│           │   ├── 安装位置
│           │   ├── 驱动选择
│           │   └── 驱动配置
│           └── 右侧预览 (TemplatePreview)
└── Footer
    ├── 上一步 (Step 2 显示)
    └── 创建 / 取消
```

### 2.2 模板选择步骤

**新建**: `web-lit/src/components/template-card.ts`

```typescript
@customElement('template-card')
export class TemplateCard extends LitElement {
  @property({ type: Object }) template!: ProcessedDeviceTemplate
  @property({ type: Function }) onUse!: (t: ProcessedDeviceTemplate) => void
}
```

**卡片内容**:
- 分类图标 (emoji)
- 模板名称
- 描述 (2 行省略)
- 标签: 协议、驱动、版本
- 厂商、设备类型

**分类 Tab**: 全部 / 传感器 / 控制器 / 摄像头 / 网关 / 其他

### 2.3 模板预览

**新建**: `web-lit/src/components/template-preview.ts`

```typescript
@customElement('template-preview')
export class TemplatePreview extends LitElement {
  @property({ type: Object }) template!: ProcessedDeviceTemplate
}
```

**内容**:
- Tab 1: 属性列表 (名称、类型、单位、读写)
- Tab 2: 命令列表 (名称、描述、参数)

### 2.4 设备信息表单

**新建**: `web-lit/src/components/device-info-form.ts`

**字段**:

| 字段 | 必填 | 说明 |
|-----|------|-----|
| 设备名称 | 是 | 2-50 字符 |
| 设备描述 | 否 | Textarea |
| 设备地址 | 否* | *根据模板决定 |
| 安装位置 | 否 | 新增字段 |
| 驱动 | 否 | 下拉选择 |
| 驱动配置 | 否* | *根据驱动决定 |

### 2.5 驱动配置动态表单

**修改**: `web-lit/src/services/drivers.ts` (扩展类型)

```typescript
interface DriverConfigOption {
  name: string
  label: string
  type: 'string' | 'number' | 'boolean' | 'select'
  required: boolean
  defaultValue?: string
  description?: string
  options?: string[]  // for select type
}
```

**实时验证**: 输入时检查必填项，红色边框 + 错误提示

### 2.6 模板数据流

```typescript
// 加载模板
const { data: templates } = await templateApi.getTemplates()

// 转换格式
const processed = templates.map(transformDeviceTemplate)

// 搜索过滤
const filtered = processed.filter(t =>
  t.name.includes(query) ||
  t.displayName['zh'].includes(query)
)

// 分类过滤
const byCategory = filtered.groupBy(t => t.category)
```

---

## Phase 3: 设备详情页增强

### 3.1 属性曲线弹窗

**新建**: `web-lit/src/components/property-chart-dialog.ts`

```typescript
@customElement('property-chart-dialog')
export class PropertyChartDialog extends LitElement {
  @property({ type: Object }) property!: DeviceProperty
  @property({ type: Boolean }) open = false
  @property({ type: String }) deviceId = ''
}
```

**功能**:
- 时间范围选择: 1小时 / 6小时 / 24小时 / 7天 / 30天 / 自定义
- 自动轮询: 每 10 秒刷新
- 曲线渲染: 使用 uPlot
- 数据显示: 当前值、单位、时间范围

### 3.2 指令执行弹窗

**新建**: `web-lit/src/components/command-execute-dialog.ts`

```typescript
@customElement('command-execute-dialog')
export class CommandExecuteDialog extends LitElement {
  @property({ type: Object }) command!: DeviceCommand
  @property({ type: Boolean }) open = false
  @property({ type: String }) deviceId = ''
}
```

**功能**:
- 指令信息展示 (ID、名称、描述)
- 参数输入表单 (动态，根据 command.parameters)
- 参数类型支持: string, number, boolean
- 执行结果反馈

### 3.3 自动刷新

**修改**: `device-detail-page.ts`

```typescript
// connectedCallback 中
this.refreshInterval = setInterval(() => {
  this.loadDevice(this.deviceId)
}, 3000)

// disconnectedCallback 中
clearInterval(this.refreshInterval)

// 手动刷新按钮
async handleRefresh() {
  await this.loadDevice(this.deviceId)
}
```

### 3.4 布局优化

**修改**: `device-detail-page.ts`

- 属性列表增加迷你曲线 (SVG polyline)
- 事件列表增加级别颜色指示
- 优化信息卡片的网格布局

---

## Phase 4: 监控功能

### 4.1 监控 Tab 框架

**修改**: `device-detail-page.ts` 添加 Tab

```typescript
// Tab 配置
const tabs = [
  { id: 'overview', name: '概览' },
  { id: 'properties', name: '属性' },
  { id: 'commands', name: '指令' },
  { id: 'events', name: '事件' },
  { id: 'monitoring', name: '监控' },  // 新增
]
```

### 4.2 设备状态卡片

**新建**: `web-lit/src/components/monitoring/device-status-card.ts`

```typescript
@customElement('device-status-card')
export class DeviceStatusCard extends LitElement {
  @property({ type: Object }) status!: DeviceStatus
  @property({ type: Object }) metrics!: DeviceMetrics
}
```

**显示**:
- 在线/离线状态 + 时长
- 连接质量
- 最后通信时间

### 4.3 性能指标卡片

**新建**: `web-lit/src/components/monitoring/performance-metrics-card.ts`

```typescript
@customElement('performance-metrics-card')
export class PerformanceMetricsCard extends LitElement {
  @property({ type: Array }) metrics!: PerformanceMetric[]
}
```

**指标类型**:
- CPU 使用率
- 内存使用率
- 网络流量
- 响应时间

### 4.4 性能趋势图表

**新建**: `web-lit/src/components/monitoring/performance-chart.ts`

```typescript
@customElement('performance-chart')
export class PerformanceChart extends LitElement {
  @property({ type: String }) deviceId = ''
  @property({ type: String }) metric = ''
  @property({ type: Number }) refreshInterval = 10000  // 10秒
}
```

**技术选型**: uPlot
- 包大小: ~15KB
- 渲染性能: 优秀
- 功能: 折线图、多系列、时间轴

### 4.5 性能告警列表

**新建**: `web-lit/src/components/monitoring/performance-alerts.ts`

```typescript
@customElement('performance-alerts')
export class PerformanceAlerts extends LitElement {
  @property({ type: Array }) alerts!: Alert[]
}
```

**告警级别**: info / warning / error / critical

### 4.6 追踪记录

**新建**: `web-lit/src/components/monitoring/trace-records.ts`

```typescript
@customElement('trace-records')
export class TraceRecords extends LitElement {
  @property({ type: String }) deviceId = ''
}
```

**显示**: 操作日志列表，分页支持

---

## 文件清单

### 新建文件 (15 个)

```
web-lit/src/
├── components/
│   ├── device-card.ts                    # 设备卡片 (Phase 1)
│   ├── tag-filter.ts                     # 标签筛选 (Phase 1)
│   ├── create-device-wizard.ts           # 创建设备向导 (Phase 2)
│   ├── template-card.ts                  # 模板卡片 (Phase 2)
│   ├── template-preview.ts                # 模板预览 (Phase 2)
│   ├── device-info-form.ts               # 设备信息表单 (Phase 2)
│   ├── property-chart-dialog.ts           # 属性曲线弹窗 (Phase 3)
│   ├── command-execute-dialog.ts         # 指令执行弹窗 (Phase 3)
│   └── monitoring/
│       ├── device-status-card.ts         # 状态卡片 (Phase 4)
│       ├── performance-metrics-card.ts    # 性能指标 (Phase 4)
│       ├── performance-chart.ts           # 性能图表 (Phase 4)
│       ├── performance-alerts.ts          # 告警列表 (Phase 4)
│       └── trace-records.ts              # 追踪记录 (Phase 4)
└── services/
    └── tags.ts                           # 标签服务 (Phase 1)
```

### 修改文件 (3 个)

```
web-lit/src/
├── pages/
│   ├── devices-page.ts                   # 列表页增强 (Phase 1)
│   └── device-detail-page.ts             # 详情页增强 (Phase 3)
└── services/
    └── drivers.ts                        # 扩展类型 (Phase 2)
```

---

## API 端点

| 功能 | 方法 | 端点 |
|-----|------|------|
| 设备列表 | GET | `/api/v1/devices` |
| 创建设备 | POST | `/api/v1/devices` |
| 设备详情 | GET | `/api/v1/devices/{id}` |
| 更新设备 | PUT | `/api/v1/devices/{id}` |
| 删除设备 | DELETE | `/api/v1/devices/{id}` |
| 执行指令 | POST | `/api/v1/devices/{id}/commands/{commandId}` |
| 属性历史 | GET | `/api/v1/devices/{id}/properties/{propertyId}/history` |
| 模板列表 | GET | `/api/v1/device-templates` |
| 驱动列表 | GET | `/api/v1/drivers` |
| 驱动配置 | GET | `/api/v1/drivers/{name}/config` |
| 标签列表 | GET | `/api/v1/tags?type=device` |
| 设备监控 | GET | `/api/v1/devices/{id}/monitoring` |
| 性能数据 | GET | `/api/v1/devices/{id}/performance` |
| 告警列表 | GET | `/api/v1/devices/{id}/alerts` |
| 追踪记录 | GET | `/api/v1/devices/{id}/traces` |

---

## 技术规范

### uPlot 集成

```typescript
// performance-chart.ts
import uPlot from 'uplot'
import 'uplot/dist/uPlot.min.css'

// 初始化
const opts: uPlot.Options = {
  width: this.width,
  height: this.height,
  series: [
    {},
    { label: 'CPU', stroke: '#3b82f6' },
    { label: 'Memory', stroke: '#22c55e' },
  ],
}

this.chart = new uPlot(opts, this.data, this.shadowRoot.querySelector('.chart'))
```

### 响应式网格

```css
.device-grid {
  display: grid;
  grid-template-columns: repeat(auto-fill, minmax(280px, 1fr));
  gap: 16px;
}
```

### Shadow DOM 样式隔离

所有组件使用 Shadow DOM 封装，避免样式冲突。

---

## 实现顺序

1. Phase 1: 设备列表页增强 (1.1 → 1.2 → 1.3 → 1.4 → 1.5)
2. Phase 2: 新建设备向导 (2.1 → 2.2 → 2.3 → 2.4 → 2.5)
3. Phase 3: 设备详情页增强 (3.1 → 3.2 → 3.3 → 3.4)
4. Phase 4: 监控功能 (4.1 → 4.2 → 4.3 → 4.4 → 4.5 → 4.6)

---

## 风险和注意事项

1. **API 兼容性**: 后端 API 需返回监控相关数据
2. **uPlot 打包**: 确保 uPlot 正确引入，不影响构建
3. **性能**: 大列表需考虑虚拟滚动
4. **移动端**: 响应式布局需在移动设备上测试
