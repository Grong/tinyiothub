/**
 * 系统管理服务
 * 使用 TanStack Query 进行数据获取和状态管理
 */

import { useQuery, useMutation, useQueryClient } from '@tanstack/react-query'
import { apiGet, apiPost, apiPut } from '@/lib/api-client'
import { queryKeys } from '@/lib/query-keys'
import type { SystemFeatures } from '@/types/feature'

// 系统相关类型定义
export interface SystemConfig {
  id: string
  key: string
  value: any
  description?: string
  category: string
  updated_at: string
}

export interface SystemTask {
  id: string
  name: string
  description?: string
  cron_expression: string
  enabled: boolean
  last_run?: string
  next_run?: string
  status: 'idle' | 'running' | 'failed'
}

export interface SystemHealth {
  status: 'healthy' | 'degraded' | 'unhealthy'
  checks: {
    database: boolean
    mqtt: boolean
    storage: boolean
    memory_usage: number
    cpu_usage: number
  }
  uptime: number
  version: string
}

export interface SystemMetrics {
  timestamp: string
  cpu_usage: number
  memory_usage: number
  disk_usage: number
  network_in: number
  network_out: number
  active_connections: number
}

// API 调用函数
export const systemApi = {
  // 获取系统特性
  getFeatures: () => 
    apiGet<SystemFeatures>('system/features'),

  // 获取系统配置
  getConfig: () => 
    apiGet<SystemConfig[]>('system/config'),

  // 更新系统配置
  updateConfig: (key: string, value: any) => 
    apiPut<SystemConfig>(`system/config/${key}`, { value }),

  // 获取系统任务
  getTasks: () => 
    apiGet<SystemTask[]>('system/tasks'),

  // 启用/禁用任务
  toggleTask: (taskId: string, enabled: boolean) => 
    apiPut<SystemTask>(`system/tasks/${taskId}`, { enabled }),

  // 手动执行任务
  executeTask: (taskId: string) => 
    apiPost<boolean>(`system/tasks/${taskId}/execute`),

  // 获取系统健康状态
  getHealth: () => 
    apiGet<SystemHealth>('system/health'),

  // 获取系统指标
  getMetrics: (timeRange?: string) => 
    apiGet<SystemMetrics[]>('system/metrics', { time_range: timeRange }),

  // 系统初始化
  initialize: () => 
    apiPost<boolean>('system/initialize'),
}

// React Query Hooks

/**
 * 获取系统特性
 */
export const useSystemFeatures = () => {
  return useQuery({
    queryKey: queryKeys.system.features(),
    queryFn: systemApi.getFeatures,
    staleTime: 1000 * 60 * 30, // 30分钟
  })
}

/**
 * 获取系统配置
 */
export const useSystemConfig = () => {
  return useQuery({
    queryKey: queryKeys.system.config(),
    queryFn: systemApi.getConfig,
    staleTime: 1000 * 60 * 10, // 10分钟
  })
}

/**
 * 获取系统任务
 */
export const useSystemTasks = () => {
  return useQuery({
    queryKey: queryKeys.system.tasks(),
    queryFn: systemApi.getTasks,
    staleTime: 1000 * 60 * 5, // 5分钟
  })
}

/**
 * 获取系统健康状态
 */
export const useSystemHealth = () => {
  return useQuery({
    queryKey: queryKeys.monitoring.health(),
    queryFn: systemApi.getHealth,
    staleTime: 1000 * 30, // 30秒
    refetchInterval: 1000 * 60, // 每分钟刷新
  })
}

/**
 * 获取系统指标
 */
export const useSystemMetrics = (timeRange?: string) => {
  return useQuery({
    queryKey: [...queryKeys.monitoring.metrics(), { timeRange }],
    queryFn: () => systemApi.getMetrics(timeRange),
    staleTime: 1000 * 30, // 30秒
    refetchInterval: 1000 * 60, // 每分钟刷新
  })
}

/**
 * 更新系统配置
 */
export const useUpdateSystemConfig = () => {
  const queryClient = useQueryClient()

  return useMutation({
    mutationFn: ({ key, value }: { key: string; value: any }) =>
      systemApi.updateConfig(key, value),
    onSuccess: () => {
      // 刷新系统配置
      queryClient.invalidateQueries({ queryKey: queryKeys.system.config() })
    },
  })
}

/**
 * 切换任务状态
 */
export const useToggleTask = () => {
  const queryClient = useQueryClient()

  return useMutation({
    mutationFn: ({ taskId, enabled }: { taskId: string; enabled: boolean }) =>
      systemApi.toggleTask(taskId, enabled),
    onSuccess: () => {
      // 刷新任务列表
      queryClient.invalidateQueries({ queryKey: queryKeys.system.tasks() })
    },
  })
}

/**
 * 执行任务
 */
export const useExecuteTask = () => {
  const queryClient = useQueryClient()

  return useMutation({
    mutationFn: systemApi.executeTask,
    onSuccess: () => {
      // 刷新任务列表
      queryClient.invalidateQueries({ queryKey: queryKeys.system.tasks() })
    },
  })
}

/**
 * 系统初始化
 */
export const useSystemInitialize = () => {
  const queryClient = useQueryClient()

  return useMutation({
    mutationFn: systemApi.initialize,
    onSuccess: () => {
      // 刷新所有系统相关查询
      queryClient.invalidateQueries({ queryKey: queryKeys.system.all })
    },
  })
}