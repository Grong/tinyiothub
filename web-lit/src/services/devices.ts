/**
 * 设备管理服务 - Pure async API functions
 */

import { apiGet, apiPost, apiPut, apiDelete } from '../lib/api-client'
import type { PaginatedResponse } from '../lib/api-client'
import type { Device, DeviceProperty, DeviceCommand } from '../types/device'

// Re-export types from canonical source
export type { Device, DeviceProperty, DeviceCommand } from '../types/device'

// Services-layer specific types (not in types/device.ts)
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
  isCreatedByMe?: boolean
  tagIds?: string[]
}

export interface CreateDeviceRequest {
  name: string
  displayName?: string
  description?: string
  protocol?: string
  address?: string
  position?: string
  driverName?: string
  driverOptions?: string
  tags?: string[]
  type?: string
  propertyValues?: Record<string, string>
  enabledCommands?: string[]
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

// Phase 4: Monitoring Types (camelCase — API client converts snake_case)
export interface DeviceOnlineStatus {
  deviceId: string
  isOnline: boolean
  connectionQuality?: number
  lastCheck: string
}

export interface DeviceMetrics {
  deviceId: string
  cpuUsage?: number
  memoryUsage?: number
  networkIn?: number
  networkOut?: number
  diskUsage?: number
  uptime?: number
  temperature?: number
  timestamp: string
}

export interface PerformanceMetric {
  name: string
  value: number
  unit?: string
  timestamp: string
}

export interface PerformanceDataPoint {
  timestamp: number
  value: number
}

export interface PerformanceHistory {
  metric: string
  data: PerformanceDataPoint[]
}

export interface PerformanceAlert {
  id: string
  deviceId: string
  alertType: string
  level: 'info' | 'warning' | 'error' | 'critical'
  message: string
  metricName?: string
  metricValue?: number
  threshold?: number
  triggeredAt: string
  acknowledged?: boolean
}

export interface DeviceTrace {
  id: string
  deviceId: string
  traceType: string
  level: string
  category: string
  title: string
  message: string
  details?: Record<string, any>
  source?: string
  userId?: string
  sessionId?: string
  createdAt: string
}

export interface DeviceTraceStatistics {
  device_id: string
  total_traces: number
  by_level: Record<string, number>
  by_type: Record<string, number>
  recent_24h: number
  recent_7d: number
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

  // Phase 4: Monitoring APIs
  getDeviceStatus: (deviceId: string) =>
    apiGet<DeviceOnlineStatus>(`devices/${deviceId}/status`),

  getDeviceMetrics: (deviceId: string) =>
    apiGet<DeviceMetrics>(`devices/${deviceId}/metrics`),

  getDevicePerformance: (deviceId: string, hours?: number) =>
    apiGet<PerformanceHistory>(`devices/${deviceId}/performance`, hours ? { hours } : undefined),

  getDevicePerformanceAlerts: (deviceId: string) =>
    apiGet<PerformanceAlert[]>(`devices/${deviceId}/performance/alerts`),

  getDeviceTraces: (deviceId: string, params?: { limit?: number; offset?: number; trace_types?: string[] }) =>
    apiGet<DeviceTrace[]>(`devices/${deviceId}/traces`, params),

  getDeviceTraceStatistics: (deviceId: string, days?: number) =>
    apiGet<DeviceTraceStatistics>(`devices/${deviceId}/traces/statistics`, days ? { days } : undefined),
}
