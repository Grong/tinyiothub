/**
 * 告警管理服务
 * 使用 TanStack Query 进行数据获取和状态管理
 * 与后端 API 保持一致
 */

import { useQuery, useMutation, useQueryClient } from '@tanstack/react-query'
import { apiGet, apiPost, apiPut, apiDelete, type PaginatedResponse } from '@/lib/api-client'
import { queryKeys } from '@/lib/query-keys'
import type {
  Alarm,
  AlarmRule,
  AlarmStatistics,
  AlarmQueryParams,
  StatisticsQueryParams,
  CreateAlarmRuleRequest,
  UpdateAlarmRuleRequest,
  AcknowledgeRequest,
  ResolveRequest,
  BatchAcknowledgeRequest,
  BatchResolveRequest,
  BatchOperationResult,
} from '@/types/alarm'

// API 调用函数
export const alarmApi = {
  // 获取报警列表
  getAlarms: (params?: AlarmQueryParams) => 
    apiGet<PaginatedResponse<Alarm>>('alarms', params),

  // 获取报警详情
  getAlarm: (id: string) => 
    apiGet<Alarm>(`alarms/${id}`),

  // 获取报警统计
  getAlarmStatistics: (params?: StatisticsQueryParams) => 
    apiGet<AlarmStatistics>('alarms/statistics', params),

  // 确认报警
  acknowledgeAlarm: (id: string, data?: AcknowledgeRequest) => 
    apiPost<void>(`alarms/${id}/acknowledge`, data),

  // 解决报警
  resolveAlarm: (id: string, data: ResolveRequest) => 
    apiPost<void>(`alarms/${id}/resolve`, data),

  // 批量确认报警
  batchAcknowledgeAlarms: (data: BatchAcknowledgeRequest) => 
    apiPost<BatchOperationResult>('alarms/batch-acknowledge', data),

  // 批量解决报警
  batchResolveAlarms: (data: BatchResolveRequest) => 
    apiPost<BatchOperationResult>('alarms/batch-resolve', data),

  // 获取报警规则列表
  getAlarmRules: (params?: { deviceId?: string }) => 
    apiGet<AlarmRule[]>('alarm-rules', params),

  // 获取报警规则详情
  getAlarmRule: (id: string) => 
    apiGet<AlarmRule>(`alarm-rules/${id}`),

  // 创建报警规则
  createAlarmRule: (data: CreateAlarmRuleRequest) => 
    apiPost<AlarmRule>('alarm-rules', data),

  // 更新报警规则
  updateAlarmRule: (id: string, data: UpdateAlarmRuleRequest) => 
    apiPut<AlarmRule>(`alarm-rules/${id}`, data),

  // 删除报警规则
  deleteAlarmRule: (id: string) => 
    apiDelete<void>(`alarm-rules/${id}`),

  // 启用/禁用报警规则
  toggleAlarmRule: (id: string, enabled: boolean) => 
    apiPost<void>(`alarm-rules/${id}/toggle`, { enabled }),
}

// React Query Hooks

/**
 * 获取报警列表
 */
export const useAlarms = (params?: AlarmQueryParams) => {
  return useQuery({
    queryKey: queryKeys.alarms.list(params || {}),
    queryFn: async () => {
      const response = await alarmApi.getAlarms(params)
      return response.result
    },
    staleTime: 1000 * 30, // 30秒
  })
}

/**
 * 获取报警详情
 */
export const useAlarm = (id: string, enabled = true) => {
  return useQuery({
    queryKey: queryKeys.alarms.detail(id),
    queryFn: async () => {
      const response = await alarmApi.getAlarm(id)
      return response.result
    },
    enabled: enabled && !!id,
    staleTime: 1000 * 30, // 30秒
  })
}

/**
 * 获取报警统计
 */
export const useAlarmStatistics = (params?: StatisticsQueryParams) => {
  return useQuery({
    queryKey: [...queryKeys.alarms.all, 'statistics', params || {}],
    queryFn: async () => {
      const response = await alarmApi.getAlarmStatistics(params)
      return response.result
    },
    staleTime: 1000 * 60, // 1分钟
  })
}

/**
 * 获取报警规则列表
 */
export const useAlarmRules = (params?: { deviceId?: string }) => {
  return useQuery({
    queryKey: params?.deviceId 
      ? [...queryKeys.alarms.rules(), params.deviceId]
      : queryKeys.alarms.rules(),
    queryFn: async () => {
      const response = await alarmApi.getAlarmRules(params)
      return response.result || []
    },
    staleTime: 1000 * 60 * 5, // 5分钟
  })
}

/**
 * 获取报警规则详情
 */
export const useAlarmRule = (id: string, enabled = true) => {
  return useQuery({
    queryKey: [...queryKeys.alarms.rules(), id],
    queryFn: async () => {
      const response = await alarmApi.getAlarmRule(id)
      return response.result
    },
    enabled: enabled && !!id,
    staleTime: 1000 * 60 * 5, // 5分钟
  })
}

/**
 * 确认报警
 */
export const useAcknowledgeAlarm = () => {
  const queryClient = useQueryClient()

  return useMutation({
    mutationFn: ({ id, data }: { id: string; data?: AcknowledgeRequest }) =>
      alarmApi.acknowledgeAlarm(id, data),
    onSuccess: (response, { id }) => {
      queryClient.invalidateQueries({ queryKey: queryKeys.alarms.detail(id) })
      queryClient.invalidateQueries({ queryKey: queryKeys.alarms.lists() })
      return response.result
    },
  })
}

/**
 * 解决报警
 */
export const useResolveAlarm = () => {
  const queryClient = useQueryClient()

  return useMutation({
    mutationFn: ({ id, data }: { id: string; data: ResolveRequest }) =>
      alarmApi.resolveAlarm(id, data),
    onSuccess: (response, { id }) => {
      queryClient.invalidateQueries({ queryKey: queryKeys.alarms.detail(id) })
      queryClient.invalidateQueries({ queryKey: queryKeys.alarms.lists() })
      return response.result
    },
  })
}

/**
 * 批量确认报警
 */
export const useBatchAcknowledgeAlarms = () => {
  const queryClient = useQueryClient()

  return useMutation({
    mutationFn: (data: BatchAcknowledgeRequest) =>
      alarmApi.batchAcknowledgeAlarms(data),
    onSuccess: (response) => {
      queryClient.invalidateQueries({ queryKey: queryKeys.alarms.lists() })
      return response.result
    },
  })
}

/**
 * 批量解决报警
 */
export const useBatchResolveAlarms = () => {
  const queryClient = useQueryClient()

  return useMutation({
    mutationFn: (data: BatchResolveRequest) =>
      alarmApi.batchResolveAlarms(data),
    onSuccess: (response) => {
      queryClient.invalidateQueries({ queryKey: queryKeys.alarms.lists() })
      return response.result
    },
  })
}

/**
 * 创建报警规则
 */
export const useCreateAlarmRule = () => {
  const queryClient = useQueryClient()

  return useMutation({
    mutationFn: alarmApi.createAlarmRule,
    onSuccess: (response) => {
      queryClient.invalidateQueries({ queryKey: queryKeys.alarms.rules() })
      return response.result
    },
  })
}

/**
 * 更新报警规则
 */
export const useUpdateAlarmRule = () => {
  const queryClient = useQueryClient()

  return useMutation({
    mutationFn: ({ id, data }: { id: string; data: UpdateAlarmRuleRequest }) =>
      alarmApi.updateAlarmRule(id, data),
    onSuccess: (response, { id }) => {
      queryClient.invalidateQueries({ queryKey: [...queryKeys.alarms.rules(), id] })
      queryClient.invalidateQueries({ queryKey: queryKeys.alarms.rules() })
      return response.result
    },
  })
}

/**
 * 删除报警规则
 */
export const useDeleteAlarmRule = () => {
  const queryClient = useQueryClient()

  return useMutation({
    mutationFn: alarmApi.deleteAlarmRule,
    onSuccess: (response) => {
      queryClient.invalidateQueries({ queryKey: queryKeys.alarms.rules() })
      return response.result
    },
  })
}

/**
 * 切换报警规则状态
 */
export const useToggleAlarmRule = () => {
  const queryClient = useQueryClient()

  return useMutation({
    mutationFn: ({ id, enabled }: { id: string; enabled: boolean }) =>
      alarmApi.toggleAlarmRule(id, enabled),
    onSuccess: (response, { id }) => {
      queryClient.invalidateQueries({ queryKey: [...queryKeys.alarms.rules(), id] })
      queryClient.invalidateQueries({ queryKey: queryKeys.alarms.rules() })
      return response.result
    },
  })
}