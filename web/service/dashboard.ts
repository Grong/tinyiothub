/**
 * Dashboard 数据服务
 */

import { useQuery } from '@tanstack/react-query'
import { apiGet } from '@/lib/api-client'
import { queryKeys } from '@/lib/query-keys'
import type { 
  DashboardData,
  DashboardStats,
  DeviceStatusDistribution,
  DataTrend,
  ProtocolUsage,
  RecentAlarm,
  DashboardMetrics,
  QuickDevice
} from '@/types'

// API 调用函数
export const dashboardApi = {
  // 获取 Dashboard 汇总数据
  getDashboardData: () => 
    apiGet<DashboardData>('monitoring/dashboard'),

  // 获取系统统计信息
  getStats: () => 
    apiGet<DashboardStats>('monitoring/stats'),

  // 获取设备状态分布
  getDeviceDistribution: () => 
    apiGet<DeviceStatusDistribution>('devices/distribution'),

  // 获取数据趋势
  getDataTrends: (period: '24h' | '7d' | '30d' = '24h') => 
    apiGet<DataTrend[]>('monitoring/trends', { period }),

  // 获取协议使用统计
  getProtocolUsage: () => 
    apiGet<ProtocolUsage[]>('monitoring/protocols'),

  // 获取最新告警
  getRecentAlarms: (limit: number = 10) => 
    apiGet<RecentAlarm[]>('alarms/recent', { limit }),

  // 获取系统性能指标
  getSystemMetrics: () => 
    apiGet<DashboardMetrics>('monitoring/metrics'),

  // 获取关键设备
  getQuickDevices: (limit: number = 8) => 
    apiGet<QuickDevice[]>('devices/quick', { limit }),
}

// React Query Hooks

/**
 * 获取完整的 Dashboard 数据
 */
export const useDashboardData = () => {
  return useQuery({
    queryKey: queryKeys.dashboard.all,
    queryFn: dashboardApi.getDashboardData,
    staleTime: 1000 * 30, // 30秒
    refetchInterval: 1000 * 60, // 1分钟自动刷新
  })
}

/**
 * 获取系统统计信息
 */
export const useDashboardStats = () => {
  return useQuery({
    queryKey: queryKeys.dashboard.stats,
    queryFn: dashboardApi.getStats,
    staleTime: 1000 * 30, // 30秒
    refetchInterval: 1000 * 60, // 1分钟自动刷新
  })
}

/**
 * 获取设备状态分布
 */
export const useDeviceDistribution = () => {
  return useQuery({
    queryKey: queryKeys.dashboard.deviceDistribution,
    queryFn: dashboardApi.getDeviceDistribution,
    staleTime: 1000 * 30, // 30秒
    refetchInterval: 1000 * 60, // 1分钟自动刷新
  })
}

/**
 * 获取数据趋势
 */
export const useDataTrends = (period: '24h' | '7d' | '30d' = '24h') => {
  return useQuery({
    queryKey: queryKeys.dashboard.trends(period),
    queryFn: () => dashboardApi.getDataTrends(period),
    staleTime: 1000 * 60, // 1分钟
    refetchInterval: 1000 * 60 * 5, // 5分钟自动刷新
  })
}

/**
 * 获取协议使用统计
 */
export const useProtocolUsage = () => {
  return useQuery({
    queryKey: queryKeys.dashboard.protocols,
    queryFn: dashboardApi.getProtocolUsage,
    staleTime: 1000 * 60 * 5, // 5分钟
    refetchInterval: 1000 * 60 * 10, // 10分钟自动刷新
  })
}

/**
 * 获取最新告警
 */
export const useRecentAlarms = (limit: number = 10) => {
  return useQuery({
    queryKey: queryKeys.dashboard.alarms(limit),
    queryFn: () => dashboardApi.getRecentAlarms(limit),
    staleTime: 1000 * 15, // 15秒
    refetchInterval: 1000 * 30, // 30秒自动刷新
  })
}

/**
 * 获取系统性能指标
 */
export const useSystemMetrics = () => {
  return useQuery({
    queryKey: queryKeys.dashboard.metrics,
    queryFn: dashboardApi.getSystemMetrics,
    staleTime: 1000 * 10, // 10秒
    refetchInterval: 1000 * 30, // 30秒自动刷新
  })
}

/**
 * 获取关键设备
 */
export const useQuickDevices = (limit: number = 8) => {
  return useQuery({
    queryKey: queryKeys.dashboard.quickDevices(limit),
    queryFn: () => dashboardApi.getQuickDevices(limit),
    staleTime: 1000 * 30, // 30秒
    refetchInterval: 1000 * 60, // 1分钟自动刷新
  })
}