/**
 * Dashboard 数据服务 - Pure async API functions
 */

import { apiGet } from '../lib/api-client'

// Types
export interface DashboardData {
  stats: DashboardStats
  deviceDistribution: DeviceStatusDistribution
  recentAlarms: RecentAlarm[]
  quickDevices: QuickDevice[]
}

export interface DashboardStats {
  totalDevices: number
  onlineDevices: number
  offlineDevices: number
  activeAlarms: number
  criticalAlarms: number
}

export interface DeviceStatusDistribution {
  online: number
  offline: number
  warning: number
  error: number
}

export interface DataTrend {
  timestamp: string
  value: number
  label?: string
}

export interface ProtocolUsage {
  protocol: string
  count: number
  percentage: number
}

export interface RecentAlarm {
  id: string
  deviceId: string
  deviceName: string
  alarmType: string
  level: number
  message: string
  timestamp: string
  acknowledged: boolean
}

export interface DashboardMetrics {
  cpu: number
  memory: number
  disk: number
  network: {
    inbound: number
    outbound: number
  }
}

export interface QuickDevice {
  id: string
  name: string
  status: string
  protocol: string
  lastSeen: string
}

// Pure async API functions
export const dashboardApi = {
  getDashboardData: () =>
    apiGet<DashboardData>('monitoring/dashboard'),

  getStats: () =>
    apiGet<DashboardStats>('monitoring/stats'),

  getDeviceDistribution: () =>
    apiGet<DeviceStatusDistribution>('devices/distribution'),

  getDataTrends: (period: '24h' | '7d' | '30d' = '24h') =>
    apiGet<DataTrend[]>('monitoring/trends', { period }),

  getProtocolUsage: () =>
    apiGet<ProtocolUsage[]>('monitoring/protocols'),

  getRecentAlarms: (limit: number = 10) =>
    apiGet<RecentAlarm[]>('alarms/recent', { limit }),

  getSystemMetrics: () =>
    apiGet<DashboardMetrics>('monitoring/metrics'),

  getQuickDevices: (limit: number = 8) =>
    apiGet<QuickDevice[]>('devices/quick', { limit }),
}
