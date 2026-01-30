import { apiGet, apiPost } from '@/lib/api-client'
import { useQuery, useMutation, useQueryClient } from '@tanstack/react-query'
import { queryKeys } from '@/lib/query-keys'

// 1. 定义类型接口
export interface DevicePerformanceMetrics {
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

export interface DeviceOnlineStatus {
  deviceId: string
  isOnline: boolean
  connectionQuality?: number
  lastCheck: string
}

export interface DeviceMetrics {
  totalProperties: number
  onlineProperties: number
  offlineProperties: number
  totalCommands: number
  totalEvents: number
  activeAlarms: number
}

export interface PerformanceAlert {
  deviceId: string
  alertType: string // high_cpu, high_memory, high_latency, slow_response, high_error_rate, low_uptime
  severity: string // warning, critical
  message: string
  currentValue: number
  threshold: number
  timestamp: string
}

export interface DeviceTrace {
  id: string
  deviceId: string
  traceType: string // operation, status_change, error, warning, info
  level: string // debug, info, warn, error, critical
  category: string // system, user, device, network, performance
  title: string
  message: string
  details?: string // JSON 格式的详细信息
  source?: string // api, system, device, scheduler
  userId?: string
  sessionId?: string
  createdAt: string
}

export interface DeviceTraceStatistics {
  deviceId: string
  totalTraces: number
  errorTraces: number
  warningTraces: number
  infoTraces: number
  daysRange: number
  lastTraceTime?: string
  lastUpdated: string
}

export interface RecordTraceRequest {
  traceType: string
  level: string
  category: string
  title: string
  message: string
  details?: any
  source?: string
  userId?: string
  sessionId?: string
}

export interface TraceQuery {
  traceTypes?: string[]
  levels?: string[]
  limit?: number
  offset?: number
}

export interface ClearTracesRequest {
  beforeDate?: string
  traceTypes?: string[]
}

// 2. API调用函数
export const deviceMonitoringApi = {
  // 设备状态相关
  getDeviceStatus: (deviceId: string) => 
    apiGet<DeviceOnlineStatus>(`devices/${deviceId}/status`),
    
  getDeviceMetrics: (deviceId: string) => 
    apiGet<DeviceMetrics>(`devices/${deviceId}/metrics`),

  // 性能监控相关
  getDevicePerformance: (deviceId: string) => 
    apiGet<DevicePerformanceMetrics>(`devices/${deviceId}/performance`),
    
  getDevicePerformanceHistory: (deviceId: string, hours?: number) => 
    apiGet<DevicePerformanceMetrics[]>(`devices/${deviceId}/performance/history`, { hours }),
    
  getDevicePerformanceAlerts: (deviceId: string) => 
    apiGet<PerformanceAlert[]>(`devices/${deviceId}/performance/alerts`),

  // 追踪记录相关
  getDeviceTraces: (deviceId: string, params?: TraceQuery) => 
    apiGet<DeviceTrace[]>(`devices/${deviceId}/traces`, params),
    
  recordDeviceTrace: (deviceId: string, data: RecordTraceRequest) => 
    apiPost<string>(`devices/${deviceId}/traces`, data),
    
  getDeviceTraceStatistics: (deviceId: string, days?: number) => 
    apiGet<DeviceTraceStatistics>(`devices/${deviceId}/traces/summary`, { days }),
    
  clearDeviceTraces: (deviceId: string, data: ClearTracesRequest) => 
    apiPost<number>(`devices/${deviceId}/traces/clear`, data),
}

// 3. React Query Hooks

// 设备状态 hooks
export const useDeviceStatus = (deviceId: string, enabled = true) => {
  return useQuery({
    queryKey: queryKeys.devices.status(deviceId),
    queryFn: async () => {
      const response = await deviceMonitoringApi.getDeviceStatus(deviceId)
      return response.result
    },
    enabled: enabled && !!deviceId,
    refetchInterval: 30000, // 30秒刷新一次
  })
}

export const useDeviceMetrics = (deviceId: string, enabled = true) => {
  return useQuery({
    queryKey: queryKeys.devices.metrics(deviceId),
    queryFn: async () => {
      const response = await deviceMonitoringApi.getDeviceMetrics(deviceId)
      return response.result
    },
    enabled: enabled && !!deviceId,
    refetchInterval: 60000, // 1分钟刷新一次
  })
}

// 性能监控 hooks
export const useDevicePerformance = (deviceId: string, enabled = true) => {
  return useQuery({
    queryKey: queryKeys.devices.performance(deviceId),
    queryFn: async () => {
      const response = await deviceMonitoringApi.getDevicePerformance(deviceId)
      return response.result
    },
    enabled: enabled && !!deviceId,
    refetchInterval: 15000, // 15秒刷新一次
  })
}

export const useDevicePerformanceHistory = (deviceId: string, hours = 24, enabled = true) => {
  return useQuery({
    queryKey: queryKeys.devices.performanceHistory(deviceId, hours),
    queryFn: async () => {
      const response = await deviceMonitoringApi.getDevicePerformanceHistory(deviceId, hours)
      return response.result || []
    },
    enabled: enabled && !!deviceId,
    refetchInterval: 300000, // 5分钟刷新一次
  })
}

export const useDevicePerformanceAlerts = (deviceId: string, enabled = true) => {
  return useQuery({
    queryKey: queryKeys.devices.performanceAlerts(deviceId),
    queryFn: async () => {
      const response = await deviceMonitoringApi.getDevicePerformanceAlerts(deviceId)
      return response.result || []
    },
    enabled: enabled && !!deviceId,
    refetchInterval: 30000, // 30秒刷新一次
  })
}

// 追踪记录 hooks
export const useDeviceTraces = (deviceId: string, params?: TraceQuery, enabled = true) => {
  return useQuery({
    queryKey: queryKeys.devices.traces(deviceId, params),
    queryFn: async () => {
      const response = await deviceMonitoringApi.getDeviceTraces(deviceId, params)
      return response.result || []
    },
    enabled: enabled && !!deviceId,
  })
}

export const useDeviceTraceStatistics = (deviceId: string, days = 7, enabled = true) => {
  return useQuery({
    queryKey: queryKeys.devices.traceStatistics(deviceId, days),
    queryFn: async () => {
      const response = await deviceMonitoringApi.getDeviceTraceStatistics(deviceId, days)
      return response.result
    },
    enabled: enabled && !!deviceId,
    refetchInterval: 300000, // 5分钟刷新一次
  })
}

// Mutation hooks
export const useRecordDeviceTrace = () => {
  const queryClient = useQueryClient()
  
  return useMutation({
    mutationFn: ({ deviceId, data }: { deviceId: string; data: RecordTraceRequest }) =>
      deviceMonitoringApi.recordDeviceTrace(deviceId, data),
    onSuccess: (response, { deviceId }) => {
      // 刷新相关查询
      queryClient.invalidateQueries({ queryKey: queryKeys.devices.traces(deviceId) })
      queryClient.invalidateQueries({ queryKey: queryKeys.devices.traceStatistics(deviceId) })
      return response.result
    },
  })
}

export const useClearDeviceTraces = () => {
  const queryClient = useQueryClient()
  
  return useMutation({
    mutationFn: ({ deviceId, data }: { deviceId: string; data: ClearTracesRequest }) =>
      deviceMonitoringApi.clearDeviceTraces(deviceId, data),
    onSuccess: (response, { deviceId }) => {
      // 刷新相关查询
      queryClient.invalidateQueries({ queryKey: queryKeys.devices.traces(deviceId) })
      queryClient.invalidateQueries({ queryKey: queryKeys.devices.traceStatistics(deviceId) })
      return response.result
    },
  })
}