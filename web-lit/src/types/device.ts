/**
 * 设备相关类型定义
 * 前端统一使用 camelCase 命名
 */

import type { Tag } from './tag'

export interface Device {
  id: string
  name: string
  displayName?: string
  deviceType?: string
  address?: string
  description?: string
  position?: string
  driverName?: string
  deviceModel?: string
  protocolType?: string
  factoryName?: string
  linkedData?: string
  driverOptions?: string
  state?: number
  parentId?: string
  productId?: string
  organizationId?: string
  createdAt?: string
  updatedAt?: string
  // 计算属性
  status?: 'online' | 'offline' | 'error' | 'maintenance'
  tags?: Tag[]
  properties?: DeviceProperty[]
  productName?: string
}

export interface DeviceProperty {
  id: string
  deviceId: string
  name: string
  displayName?: string
  value: any // 保持兼容性，映射到 current_value
  currentValue?: any // 新增：实时值字段，映射到 current_value
  dataType: string
  unit?: string
  description?: string
  updatedAt: string // 映射到 updated_at
  lastUpdateTime?: string // 新增：最后更新时间，映射到 updated_at
  alarmStatus?: number // 告警状态：0=正常，1=告警，2=高告警，映射到 alarm_status
  isReadOnly?: boolean // 映射到 is_read_only
  minValue?: number // 映射到 min_value
  maxValue?: number // 映射到 max_value
}

export interface DeviceCommand {
  id: string
  deviceId: string
  name: string
  description?: string
  parameters: Record<string, any>
  createdAt: string
}

export interface DeviceAlarm {
  id: string
  deviceId: string
  deviceName: string
  level: 'info' | 'warning' | 'error' | 'critical'
  message: string
  status: 'active' | 'acknowledged' | 'resolved'
  createdAt: string
  acknowledgedAt?: string
  resolvedAt?: string
}

export interface DeviceListParams {
  page?: number
  pageSize?: number
  name?: string
  state?: string
  deviceType?: string
  driverName?: string
  productId?: string
  enabled?: boolean
}

export interface CreateDeviceRequest {
  name: string
  type?: string
  ipAddress?: string
  port?: number
  description?: string
  tags?: string[]
  manufacturer?: string
  model?: string
  protocol?: string
}

export interface UpdateDeviceRequest extends Partial<CreateDeviceRequest> {
  id: string
}

// 设备事件类型
export interface DeviceEvent {
  id: string
  deviceId: string
  eventType: 'alarm' | 'warning' | 'info' | 'error' | 'status_change' | 'command_executed'
  level: 'info' | 'warning' | 'error' | 'critical'
  title: string
  message: string
  data?: Record<string, any>
  source?: string
  createdAt: string
  acknowledgedAt?: string
  resolvedAt?: string
  status: 'active' | 'acknowledged' | 'resolved'
}

// 设备Profile类型（完整设备信息）
export interface DeviceProfile {
  device: Device
  isOnline: boolean
  properties: DeviceProperty[]
  commands: DeviceCommand[]
  recentEvents?: DeviceEvent[]
  overview: {
    totalProperties: number
    onlineProperties: number
    offlineProperties: number
    readonlyProperties: number
    writableProperties: number
    totalCommands: number
    totalEvents: number
    activeAlarms: number
    lastUpdateTime?: string
  }
  generatedAt: string
}