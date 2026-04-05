# web-lit 设备管理功能完整实现计划

## 概述

本计划旨在为 web-lit 实现完整的设备管理功能，包括：设备列表增强、新建设备向导、设备详情页增强、监控功能。

## 当前状态

- 设备列表页：表格布局，基本 CRUD
- 设备详情页：基础属性/指令/事件展示
- 新建设备：简单弹窗表单，无模板选择
- 监控功能：未实现

---

## Phase 1: 设备列表页增强

### 1.1 状态 Tab 补充

**文件**: `web-lit/src/pages/devices-page.ts`

**改动**:
- 添加"维护"状态选项
- 协议筛选保持不变

```typescript
// 状态 Tab 配置
const statusOptions = [
  { value: '', text: '全部' },
  { value: 'online', text: '在线' },
  { value: 'offline', text: '离线' },
  { value: 'error', text: '错误' },
  { value: 'maintenance', text: '维护' },
]
```

### 1.2 设备卡片网格布局

**文件**: 新建 `web-lit/src/components/device-card.ts`

**功能**:
- 卡片式展示设备信息
- 显示：名称、协议、地址、状态
- 响应式网格 (1-6 列)
- 点击进入详情页

**组件接口**:
```typescript
interface DeviceCardProps {
  device: Device
  onEdit: (device: Device) => void
  onDelete: (device: Device) => void
  onRefresh: () => void
}
```

### 1.3 标签筛选

**文件**: 新建 `web-lit/src/components/tag-filter.ts`

**功能**:
- 标签选择下拉
- 多选支持
- 显示已选标签

**依赖**:
- 后端 API: `/api/v1/tags` (type=device)
- 前端服务: `web-lit/src/services/tags.ts` (需新建)

### 1.4 "仅显示我创建的"筛选

**文件**: `web-lit/src/pages/devices-page.ts`

**改动**:
- 添加 Checkbox 组件
- 查询参数添加 `isCreatedByMe: boolean`
- API 调用传递该参数

### 1.5 设备列表布局切换

**文件**: `web-lit/src/pages/devices-page.ts`

**功能**:
- 支持表格/网格两种视图切换
- 默认使用网格视图
- 用户偏好保存到 localStorage

---

## Phase 2: 新建设备向导（模板选择）

### 2.1 模板选择弹窗

**文件**: 新建 `web-lit/src/components/create-device-wizard.ts`

**结构**:
```
CreateDeviceWizard
├── StepIndicator (步骤指示器)
├── TemplateSelectionStep (步骤1: 模板选择)
│   ├── 搜索框
│   ├── 分类 Tab (全部/传感器/控制器/摄像头/网关/其他)
│   └── TemplateCard 网格
└── DeviceInfoStep (步骤2: 设备信息)
    ├── 模板信息摘要
    ├── 设备名称* (必填)
    ├── 设备描述
    ├── 设备地址
    ├── 安装位置
    ├── 驱动选择下拉
    ├── 驱动配置表单 (动态)
    └── 模板详情预览 (右侧)
```

### 2.2 模板卡片组件

**文件**: 新建 `web-lit/src/components/template-card.ts`

**功能**:
- 显示：图标、名称、描述、分类
- 标签：协议、驱动、版本
- 点击选中和预览

**组件接口**:
```typescript
interface TemplateCardProps {
  template: ProcessedDeviceTemplate
  onUse: (template: ProcessedDeviceTemplate) => void
}
```

### 2.3 模板详情预览组件

**文件**: 新建 `web-lit/src/components/template-preview.ts`

**功能**:
- 属性 Tab: 列表显示属性名、类型、单位、读写权限
- 命令 Tab: 列表显示命令名、参数

### 2.4 设备信息表单

**文件**: 新建 `web-lit/src/components/device-info-form.ts`

**功能**:
- 设备名称输入 (必填, 2-50字符)
- 设备描述 Textarea
- 设备地址输入
- 安装位置输入
- 驱动选择 (下拉)
- 驱动配置 (动态表单，根据选中驱动加载)

### 2.5 驱动配置动态表单

**文件**: `web-lit/src/services/drivers.ts` (已存在)

**功能扩展**:
- 根据 `driverName` 加载配置选项
- 支持类型: string, number, boolean, select
- 必填项验证
- 默认值填充

### 2.6 模板服务

**文件**: `web-lit/src/services/templates.ts` (已存在)

**功能扩展**:
- `getTemplates()` 已存在
- `transformDeviceTemplate()` 已存在
- 添加: `getTemplateCategories()` 分类获取

---

## Phase 3: 设备详情页增强

### 3.1 属性曲线弹窗

**文件**: 新建 `web-lit/src/components/property-chart-dialog.ts`

**功能**:
- 属性历史曲线展示
- 时间范围选择 (1小时/6小时/24小时/7天/30天)
- 自定义时间范围
- 曲线渲染 (使用 SVG 或 Canvas)

**数据获取**:
```typescript
interface PropertyHistoryParams {
  deviceId: string
  propertyId: string
  startTime: string
  endTime: string
}
// API: GET /api/v1/devices/{deviceId}/properties/{propertyId}/history
```

### 3.2 指令执行弹窗

**文件**: 新建 `web-lit/src/components/command-execute-dialog.ts`

**功能**:
- 指令信息展示 (名称、描述、ID)
- 参数输入表单 (根据指令定义动态生成)
- 参数类型: string, number, boolean
- 执行确认和结果反馈

**组件接口**:
```typescript
interface CommandExecuteDialogProps {
  deviceId: string
  command: DeviceCommand
  isOpen: boolean
  onClose: () => void
  onSuccess: () => void
}
```

### 3.3 自动刷新机制

**文件**: `web-lit/src/pages/device-detail-page.ts`

**功能**:
- 每 3 秒自动刷新设备数据
- 使用 `setInterval` 或 `requestAnimationFrame`
- 后台也刷新 (页面不可见时暂停)
- 可手动刷新按钮

### 3.4 设备详情布局优化

**文件**: `web-lit/src/pages/device-detail-page.ts`

**改动**:
- 优化信息卡片布局
- 属性列表增加迷你曲线
- 事件列表增加分类图标

---

## Phase 4: 监控功能

### 4.1 监控页面组件

**文件**: 新建 `web-lit/src/pages/device-monitoring-page.ts`

**功能**:
- Tab 导航: 概览 / 性能 / 告警 / 追踪
- 设备 ID 接收 (URL 参数)

### 4.2 设备状态卡片

**文件**: 新建 `web-lit/src/components/monitoring/device-status-card.ts`

**功能**:
- 在线/离线状态
- 连接时间
- 基本指标显示

### 4.3 性能指标卡片

**文件**: 新建 `web-lit/src/components/monitoring/performance-metrics-card.ts`

**功能**:
- CPU/内存/网络等指标
- 数值和百分比展示
- 状态颜色指示

### 4.4 性能趋势图表

**文件**: 新建 `web-lit/src/components/monitoring/performance-chart.ts`

**功能**:
- 折线图展示历史趋势
- 支持多指标叠加
- 时间范围选择
- 使用 SVG 渲染

### 4.5 性能告警列表

**文件**: 新建 `web-lit/src/components/monitoring/performance-alerts.ts`

**功能**:
- 告警级别 (info/warning/error/critical)
- 告警时间和内容
- 告警状态标记

### 4.6 追踪记录列表

**文件**: 新建 `web-lit/src/components/monitoring/trace-records.ts`

**功能**:
- 操作日志列表
- 时间、操作类型、内容
- 分页支持

---

## 文件结构

```
web-lit/src/
├── pages/
│   ├── devices-page.ts          [修改] 列表页增强
│   ├── device-detail-page.ts    [修改] 详情页增强
│   └── device-monitoring-page.ts [新建] 监控页
├── components/
│   ├── device-card.ts           [新建] 设备卡片
│   ├── tag-filter.ts            [新建] 标签筛选
│   ├── create-device-wizard.ts  [新建] 创建设备向导
│   ├── template-card.ts         [新建] 模板卡片
│   ├── template-preview.ts      [新建] 模板预览
│   ├── device-info-form.ts      [新建] 设备信息表单
│   ├── property-chart-dialog.ts [新建] 属性曲线弹窗
│   ├── command-execute-dialog.ts [新建] 指令执行弹窗
│   └── monitoring/
│       ├── device-status-card.ts        [新建]
│       ├── performance-metrics-card.ts   [新建]
│       ├── performance-chart.ts          [新建]
│       ├── performance-alerts.ts         [新建]
│       └── trace-records.ts             [新建]
├── services/
│   ├── devices.ts   [修改] 添加监控 API
│   ├── drivers.ts   [修改] 扩展配置选项类型
│   └── tags.ts      [新建] 标签服务
├── lib/
│   └── navigate.ts  [已存在]
├── styles/
│   └── *.css        [修改] 添加必要样式
└── types/
    └── *.ts         [修改] 扩展类型定义
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

## 依赖关系

```
Phase 1 (设备列表)
├── tag-filter.ts → tags.ts 服务
├── device-card.ts → devices.ts 服务
└── devices-page.ts → device-card.ts + tag-filter.ts

Phase 2 (设备向导)
├── template-card.ts → templates.ts 服务
├── template-preview.ts → templates.ts 服务
├── device-info-form.ts → drivers.ts 服务
└── create-device-wizard.ts → 以上组件

Phase 3 (详情增强)
├── property-chart-dialog.ts → devices.ts 服务
├── command-execute-dialog.ts → devices.ts 服务
└── device-detail-page.ts → 以上组件 + 已有功能

Phase 4 (监控功能)
├── monitoring/*.ts → devices.ts 监控 API
└── device-monitoring-page.ts → monitoring/*.ts
```

---

## 实现顺序

1. **Phase 1**: 设备列表页增强
   - 1.1 状态 Tab 补充
   - 1.2 设备卡片组件
   - 1.3 标签服务 + 筛选
   - 1.4 "仅显示我创建的"筛选
   - 1.5 视图切换

2. **Phase 2**: 新建设备向导
   - 2.1 模板卡片组件
   - 2.2 模板预览组件
   - 2.3 设备信息表单
   - 2.4 驱动配置表单
   - 2.5 创建设备向导整合

3. **Phase 3**: 设备详情页增强
   - 3.1 属性曲线弹窗
   - 3.2 指令执行弹窗
   - 3.3 自动刷新
   - 3.4 布局优化

4. **Phase 4**: 监控功能
   - 4.1 监控页面框架
   - 4.2 状态卡片
   - 4.3 性能指标
   - 4.4 性能图表
   - 4.5 告警列表
   - 4.6 追踪记录

---

## 样式规范

遵循 `web-lit/src/styles/` 中的现有 CSS 变量：

| 变量 | 用途 |
|-----|------|
| `--bg` | 页面背景 |
| `--card` | 卡片背景 |
| `--text` | 正文文字 |
| `--muted` | 次要文字 |
| `--accent` | 主色调 |
| `--ok` | 成功/在线 |
| `--danger` | 错误/危险 |
| `--warn` | 警告 |
| `--info` | 信息 |

组件样式使用 Shadow DOM 封装，参考 `devices-page.ts` 现有实现。

---

## 测试策略

1. **单元测试**: 组件逻辑测试
2. **集成测试**: API 调用测试
3. **E2E 测试**: 完整流程测试

---

## 风险和注意事项

1. **API 兼容性**: 确保后端 API 格式与前端期望一致
2. **性能**: 大列表使用虚拟滚动优化
3. **响应式**: 移动端适配
4. **国际化**: 预留 i18n 接口
