# DeviceCard 组件

设备卡片组件，用于在设备列表中展示单个设备的信息。

## 组件结构

```
device-card/
├── index.tsx              # 主组件，组合所有子组件
├── device-header.tsx      # 设备头部（图标、名称、产品信息）
├── device-status.tsx      # 设备状态徽章
├── device-content.tsx     # 设备内容（描述、属性）
├── device-actions.tsx     # 设备操作菜单（编辑、删除）
├── device-tags.tsx        # 设备标签管理
└── status-icon.tsx        # 设备状态图标
```

## 使用方式

```typescript
import DeviceCard from '@/app/components/devices/device-card'

<DeviceCard 
  device={device} 
  onRefresh={handleRefresh} 
/>
```

## Props

### DeviceCard

| 属性 | 类型 | 必填 | 说明 |
|------|------|------|------|
| device | Device | 是 | 设备数据对象 |
| onRefresh | () => void | 否 | 刷新回调函数 |

## 子组件

### DeviceHeader
显示设备图标、名称、产品信息和最后编辑时间。

### DeviceStatus
显示设备当前状态的彩色徽章（在线/离线/错误/维护）。

### DeviceContent
显示设备描述和前3个属性，超过3个显示"+N"。

### DeviceActions
提供编辑和删除操作的下拉菜单，仅在hover时显示。

### DeviceTags
标签选择器，支持添加/移除设备标签。

### StatusIcon
根据设备状态显示对应的图标。

## 特性

- ✅ 使用 React.memo 优化性能
- ✅ 统一错误处理（useErrorHandler）
- ✅ 使用常量定义（DISPLAY_LIMITS）
- ✅ 使用工具函数（device-utils）
- ✅ 组件拆分，提高可维护性
- ✅ TypeScript 类型安全

## 改进历史

- 2026-01-25: 重构为多个子组件，应用统一错误处理
- 之前: 单一大组件（300+行）
