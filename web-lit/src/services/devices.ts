/**
 * 设备管理服务 - Pure async API functions
 */

import { apiGet, apiPost, apiPut, apiDelete } from '../lib/api-client'
import type { PaginatedResponse } from '../lib/api-client'

// Types
export interface Device {
  id: string
  name: string
  displayName?: string
  description?: string
  protocol?: string
  address?: string
  status?: 'online' | 'offline' | 'warning' | 'error'
  isOnline?: boolean
  driverName?: string
  tags?: string[]
  createdAt?: string
  updatedAt?: string
}

export interface DeviceProperty {
  id: string
  name: string
  value: any
  displayName?: string
  description?: string
  dataType?: string
  unit?: string
  readonly?: boolean
  isReadOnly?: boolean
  timestamp?: string
}

export interface DeviceAlarm {
  id: string
  deviceId: string
  deviceName?: string
  alarmType?: string
  level?: 'info' | 'warning' | 'error' | 'critical'
  message?: string
  status?: 'active' | 'acknowledged' | 'resolved'
  acknowledged?: boolean
  resolved?: boolean
  timestamp?: string
}

export interface DeviceListParams {
  page?: number
  pageSize?: number
  search?: string
  protocol?: string
  status?: string
}

export interface CreateDeviceRequest {
  name: string
  displayName?: string
  description?: string
  protocol: string
  address?: string
  // ... other device fields
}

export interface DeviceCommand {
  id: string
  name: string
  parameters?: Record<string, any>
}

export interface CommandExecution {
  id: string
  commandId: string
  commandName: string
  parameters: Record<string, any>
  status: 'pending' | 'success' | 'failed'
  result?: string
  error?: string
  executedAt: string
}

export interface DeviceEventSummary {
  id: string
  eventType: string
  level: string
  title: string
  message: string
  timestamp: string
  metadata?: Record<string, any>
}

export interface DeviceProfile {
  device: Device
  isOnline: boolean
  properties: DeviceProperty[]
  commands: DeviceCommand[]
  recentEvents: DeviceEventSummary[]
  overview: {
    totalProperties: number
    onlineProperties: number
    offlineProperties: number
    readonlyProperties: number
    writableProperties: number
    totalCommands: number
    recentEventCount: number
    criticalEventCount: number
    errorEventCount: number
    lastEventTime?: string
    updatedAt?: string
  }
  generatedAt: string
}

// Pure async API functions
export const deviceApi = {
  getDevices: (params?: DeviceListParams) =>
    apiGet<PaginatedResponse<Device>>('devices', params),

  getDevice: (id: string) =>
    apiGet<Device>(`devices/${id}`),

  getDeviceProfile: (id: string) =>
    apiGet<DeviceProfile>(`devices/${id}/profile`),

  createDevice: (data: CreateDeviceRequest) =>
    apiPost<Device>('devices', data),

  updateDevice: (id: string, data: Partial<CreateDeviceRequest>) =>
    apiPut<Device>(`devices/${id}`, data),

  deleteDevice: (id: string) =>
    apiDelete<boolean>(`devices/${id}`),

  executeCommand: (deviceId: string, commandId: string, parameters: Record<string, any>) =>
    apiPost<CommandExecution>(`devices/${deviceId}/commands/${commandId}/execute`, { parameters }),

  getDeviceAlarms: (params: { deviceId?: string } & DeviceListParams) => {
    const { deviceId, ...queryParams } = params
    const endpoint = deviceId ? `devices/${deviceId}/alarms` : 'alarms'
    return apiGet<PaginatedResponse<DeviceAlarm>>(endpoint, queryParams)
  },

  acknowledgeAlarm: (alarmId: string) =>
    apiPost<boolean>(`alarms/${alarmId}/acknowledge`),

  resolveAlarm: (alarmId: string) =>
    apiPost<boolean>(`alarms/${alarmId}/resolve`),
}
