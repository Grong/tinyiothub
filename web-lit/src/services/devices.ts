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
  status?: 'online' | 'offline' | 'warning' | 'error' | 'maintenance'
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

// Phase 4: Monitoring Types
export interface DeviceOnlineStatus {
  device_id: string
  is_online: boolean
  connection_quality?: number
  last_check: string
}

export interface DeviceMetrics {
  device_id: string
  cpu_usage?: number
  memory_usage?: number
  network_in?: number
  network_out?: number
  disk_usage?: number
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
  device_id: string
  alert_type: string
  level: 'info' | 'warning' | 'error' | 'critical'
  message: string
  metric_name?: string
  metric_value?: number
  threshold?: number
  triggered_at: string
  acknowledged?: boolean
}

export interface DeviceTrace {
  id: string
  device_id: string
  trace_type: string
  level: string
  category: string
  title: string
  message: string
  details?: Record<string, any>
  source?: string
  user_id?: string
  session_id?: string
  created_at: string
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
