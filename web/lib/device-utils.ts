/**
 * 设备相关工具函数
 */

// 设备状态常量
export const DEVICE_STATE = {
  OFFLINE: 0,
  ONLINE: 1,
  ERROR: 2,
  MAINTENANCE: 3,
} as const

export type DeviceState = typeof DEVICE_STATE[keyof typeof DEVICE_STATE]

// 设备状态类型
export type DeviceStatus = 'online' | 'offline' | 'error' | 'maintenance'

/**
 * 将后端的 state 数字转换为前端的 status 字符串
 */
export const getDeviceStatus = (state?: number): DeviceStatus => {
  const statusMap: Record<number, DeviceStatus> = {
    [DEVICE_STATE.ONLINE]: 'online',
    [DEVICE_STATE.ERROR]: 'error',
    [DEVICE_STATE.MAINTENANCE]: 'maintenance',
    [DEVICE_STATE.OFFLINE]: 'offline',
  }
  return statusMap[state ?? DEVICE_STATE.OFFLINE] || 'offline'
}

/**
 * 获取设备状态的显示文本
 */
export const getDeviceStatusText = (status: DeviceStatus, t: (key: string) => string): string => {
  return t(`status.${status}`)
}

/**
 * 获取设备状态的颜色类名
 */
export const getDeviceStatusColor = (status: DeviceStatus): string => {
  const colorMap: Record<DeviceStatus, string> = {
    online: 'bg-components-badge-bg-green-soft text-text-success',
    offline: 'bg-components-badge-bg-gray-soft text-text-tertiary',
    error: 'bg-components-badge-bg-red-soft text-text-destructive',
    maintenance: 'bg-components-badge-bg-yellow-soft text-text-warning',
  }
  return colorMap[status]
}

/**
 * 获取设备状态图标组件
 */
export const getDeviceStatusIcon = (status: DeviceStatus) => {
  // 动态导入图标以避免打包所有图标
  const icons = {
    online: () => import('@remixicon/react').then(m => m.RiWifiLine),
    offline: () => import('@remixicon/react').then(m => m.RiWifiOffLine),
    error: () => import('@remixicon/react').then(m => m.RiAlarmWarningLine),
    maintenance: () => import('@remixicon/react').then(m => m.RiSettings3Line),
  }
  return icons[status]
}

/**
 * 获取设备显示名称（优先使用 displayName，否则使用 name）
 */
export const getDeviceDisplayName = (device: { displayName?: string; name: string }): string => {
  return device.displayName || device.name
}

/**
 * 获取设备产品名称
 */
export const getDeviceProductName = (
  device: { product_name?: string; deviceType?: string },
  fallback: string = '未知产品'
): string => {
  return device.product_name || device.deviceType || fallback
}
