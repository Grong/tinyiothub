/**
 * 设备管理服务
 * 使用 TanStack Query 进行数据获取和状态管理
 */

import { useQuery, useMutation, useQueryClient } from '@tanstack/react-query'
import { apiGet, apiPost, apiPut, apiDelete, type PaginatedResponse } from '@/lib/api-client'
import { queryKeys } from '@/lib/query-keys'
import type { 
  Device, 
  DeviceProperty, 
  DeviceAlarm, 
  DeviceListParams, 
  CreateDeviceRequest,
  DeviceCommand
} from '@/types'

// 指令执行结果类型
interface CommandExecution {
  id: string
  commandId: string
  commandName: string
  parameters: Record<string, any>
  status: 'pending' | 'success' | 'failed'
  result?: string
  error?: string
  executedAt: string
}

// 设备事件摘要类型（与后端 DeviceEventSummary 匹配）
interface DeviceEventSummary {
  id: string
  eventType: string  // "Connection", "Property", "Command", "Business"
  level: string      // "Debug", "Info", "Warning", "Error", "Critical"
  title: string
  message: string
  timestamp: string
  metadata?: Record<string, any>
}

// 设备Profile类型定义（与后端返回的数据结构匹配）
interface DeviceProfile {
  device: Device
  isOnline: boolean
  properties: DeviceProperty[]
  commands: DeviceCommand[]
  recentEvents: DeviceEventSummary[]  // 最近 10 条事件
  overview: {
    totalProperties: number
    onlineProperties: number
    offlineProperties: number
    readonlyProperties: number
    writableProperties: number
    totalCommands: number
    recentEventCount: number      // 最近 24 小时事件总数
    criticalEventCount: number    // 最近 24 小时严重事件数
    errorEventCount: number        // 最近 24 小时错误事件数
    lastEventTime?: string         // 最后事件时间
    updatedAt?: string
  }
  generatedAt: string
}

// API 调用函数
export const deviceApi = {
  // 获取设备列表
  getDevices: (params?: DeviceListParams) => 
    apiGet<PaginatedResponse<Device>>('devices', params),

  // 获取设备详情
  getDevice: (id: string) => 
    apiGet<Device>(`devices/${id}`),

  // 获取设备Profile（完整信息）
  getDeviceProfile: (id: string) => 
    apiGet<DeviceProfile>(`devices/${id}/profile`),

  // 创建设备
  createDevice: (data: CreateDeviceRequest) => 
    apiPost<Device>('devices', data),

  // 更新设备
  updateDevice: (id: string, data: Partial<CreateDeviceRequest>) => 
    apiPut<Device>(`devices/${id}`, data),

  // 删除设备
  deleteDevice: (id: string) => 
    apiDelete<boolean>(`devices/${id}`),



  // 执行设备指令 - 保留此函数，因为概览页面需要执行指令
  executeCommand: (deviceId: string, commandId: string, parameters: Record<string, any>) => 
    apiPost<CommandExecution>(`devices/${deviceId}/commands/${commandId}/execute`, { parameters }),

  // 获取设备告警
  getDeviceAlarms: (params: { deviceId?: string } & DeviceListParams) => {
    const { deviceId, ...queryParams } = params
    const endpoint = deviceId ? `devices/${deviceId}/alarms` : 'alarms'
    return apiGet<PaginatedResponse<DeviceAlarm>>(endpoint, queryParams)
  },

  // 确认告警
  acknowledgeAlarm: (alarmId: string) => 
    apiPost<boolean>(`alarms/${alarmId}/acknowledge`),

  // 解决告警
  resolveAlarm: (alarmId: string) => 
    apiPost<boolean>(`alarms/${alarmId}/resolve`),
}

// React Query Hooks

/**
 * 获取设备列表
 */
export const useDevices = (params?: DeviceListParams) => {
  return useQuery({
    queryKey: queryKeys.devices.list(params || {}),
    queryFn: async () => {
      const response = await deviceApi.getDevices(params)
      if (!response.result) {
        throw new Error('Device list data is null')
      }
      return response.result // 提取 result 字段
    },
    staleTime: 1000 * 60 * 5, // 5分钟
  })
}

/**
 * 获取设备Profile（完整信息）
 */
export const useDeviceProfile = (id: string, options?: { enabled?: boolean; refetchInterval?: number; refetchIntervalInBackground?: boolean }) => {
  const { enabled = true, refetchInterval, refetchIntervalInBackground } = options || {}
  
  return useQuery({
    queryKey: queryKeys.devices.profile(id),
    queryFn: async () => {
      const response = await deviceApi.getDeviceProfile(id)
      const data = response.result
      
      // 检查数据是否存在
      if (!data) {
        throw new Error('Device profile data is null')
      }
      
      // API客户端已经自动转换了字段名（snake_case → camelCase）
      return {
        device: data.device,
        isOnline: data.isOnline ?? false,
        properties: data.properties || [],
        commands: data.commands || [],
        recentEvents: data.recentEvents || [],
        overview: {
          totalProperties: data.overview?.totalProperties ?? 0,
          onlineProperties: data.overview?.onlineProperties ?? 0,
          offlineProperties: data.overview?.offlineProperties ?? 0,
          readonlyProperties: data.overview?.readonlyProperties ?? 0,
          writableProperties: data.overview?.writableProperties ?? 0,
          totalCommands: data.overview?.totalCommands ?? 0,
          totalEvents: data.overview?.recentEventCount ?? 0, // 使用recentEventCount作为totalEvents
          activeAlarms: 0, // 暂时使用0，后续从告警API获取
          recentEventCount: data.overview?.recentEventCount ?? 0,
          criticalEventCount: data.overview?.criticalEventCount ?? 0,
          errorEventCount: data.overview?.errorEventCount ?? 0,
          lastEventTime: data.overview?.lastEventTime,
          updatedAt: data.overview?.updatedAt,
        },
        generatedAt: data.generatedAt,
      }
    },
    enabled: enabled && !!id,
    staleTime: 1000 * 60 * 2, // 2分钟
    refetchInterval,
    refetchIntervalInBackground,
  })
}

/**
 * 获取设备详情
 */
export const useDevice = (id: string, enabled = true) => {
  return useQuery({
    queryKey: queryKeys.devices.detail(id),
    queryFn: async () => {
      const response = await deviceApi.getDevice(id)
      if (!response.result) {
        throw new Error('Device data is null')
      }
      return response.result // 提取 result 字段
    },
    enabled: enabled && !!id,
    staleTime: 1000 * 60 * 5, // 5分钟
  })
}



/**
 * 获取设备告警
 */
export const useDeviceAlarms = (params: { deviceId?: string } & DeviceListParams) => {
  return useQuery({
    queryKey: queryKeys.devices.alarms(params.deviceId),
    queryFn: async () => {
      const response = await deviceApi.getDeviceAlarms(params)
      if (!response.result) {
        throw new Error('Device alarms data is null')
      }
      return response.result // 提取 result 字段
    },
    staleTime: 1000 * 60, // 1分钟
  })
}

/**
 * 创建设备
 */
export const useCreateDevice = () => {
  const queryClient = useQueryClient()

  return useMutation({
    mutationFn: deviceApi.createDevice,
    onSuccess: () => {
      // 刷新设备列表
      queryClient.invalidateQueries({ queryKey: queryKeys.devices.lists() })
    },
  })
}

/**
 * 更新设备
 */
export const useUpdateDevice = () => {
  const queryClient = useQueryClient()

  return useMutation({
    mutationFn: ({ id, data }: { id: string; data: Partial<CreateDeviceRequest> }) =>
      deviceApi.updateDevice(id, data),
    onSuccess: (_, { id }) => {
      // 刷新相关查询
      queryClient.invalidateQueries({ queryKey: queryKeys.devices.detail(id) })
      queryClient.invalidateQueries({ queryKey: queryKeys.devices.lists() })
    },
  })
}

/**
 * 删除设备
 */
export const useDeleteDevice = () => {
  const queryClient = useQueryClient()

  return useMutation({
    mutationFn: deviceApi.deleteDevice,
    onSuccess: () => {
      // 刷新设备列表
      queryClient.invalidateQueries({ queryKey: queryKeys.devices.lists() })
    },
  })
}

/**
 * 更新设备属性值
 */
export const useUpdateDeviceProperty = () => {
  const queryClient = useQueryClient()

  return useMutation({
    mutationFn: (_params: { 
      deviceId: string; 
      propertyId: string; 
      value: any 
    }) => {
      // 此功能需要通过后端API实现
      throw new Error('updateDeviceProperty API needs to be implemented')
    },
    onSuccess: (_, { deviceId }) => {
      // 刷新设备Profile获取最新状态
      queryClient.invalidateQueries({ queryKey: queryKeys.devices.profile(deviceId) })
    },
  })
}

/**
 * 执行设备指令
 */
export const useExecuteCommand = () => {
  const queryClient = useQueryClient()

  return useMutation({
    mutationFn: ({ deviceId, commandId, parameters }: { 
      deviceId: string; 
      commandId: string; 
      parameters: Record<string, any> 
    }) => deviceApi.executeCommand(deviceId, commandId, parameters),
    onSuccess: (_, { deviceId }) => {
      // 刷新设备Profile以获取最新状态
      queryClient.invalidateQueries({ queryKey: queryKeys.devices.profile(deviceId) })
    },
  })
}

/**
 * 确认告警
 */
export const useAcknowledgeAlarm = () => {
  const queryClient = useQueryClient()

  return useMutation({
    mutationFn: deviceApi.acknowledgeAlarm,
    onSuccess: () => {
      // 刷新告警列表
      queryClient.invalidateQueries({ queryKey: queryKeys.devices.all })
      queryClient.invalidateQueries({ queryKey: queryKeys.alarms.lists() })
    },
  })
}

// 导出服务对象，供组件直接调用
export const deviceService = {
  ...deviceApi,
}

// 导出类型供组件使用
export type { DeviceEventSummary, DeviceProfile }

/**
 * 解决告警
 */
export const useResolveAlarm = () => {
  const queryClient = useQueryClient()

  return useMutation({
    mutationFn: deviceApi.resolveAlarm,
    onSuccess: () => {
      // 刷新告警列表
      queryClient.invalidateQueries({ queryKey: queryKeys.devices.all })
      queryClient.invalidateQueries({ queryKey: queryKeys.alarms.lists() })
    },
  })
}