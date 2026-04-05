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

// Backend response types (snake_case after api-client camelCase conversion)
interface BackendStats {
  totalDevices: number
  onlineDevices: number
  activeAlarms: number
  systemStatus: string
  systemUptime: number
  todayMessages: number
  monthlyGrowth: { devices: number; messages: number }
}

interface BackendDistribution {
  online: number
  offline: number
  error: number
  maintenance: number
}

interface BackendRecentAlarm {
  id: string
  deviceId: string
  deviceName: string
  level: string
  message: string
  createdAt: string
  status: string
}

interface BackendQuickDevice {
  id: string
  name: string
  status: string
  lastSeen: string
  deviceType: string
}

function levelToNumber(level: string): number {
  switch (level) {
    case 'critical': return 3
    case 'error': return 3
    case 'warning': return 2
    default: return 1
  }
}

// Pure async API functions
export const dashboardApi = {
  /**
   * Compose dashboard data from existing backend endpoints:
   *   GET /monitoring/stats      → stats
   *   GET /devices/distribution  → deviceDistribution
   *   GET /alarms/recent         → recentAlarms
   *   GET /devices/quick         → quickDevices
   */
  async getDashboardData(): Promise<{ result: DashboardData | null; code: number; msg: string }> {
    const [statsRes, distRes, alarmsRes, devicesRes] = await Promise.all([
      apiGet<BackendStats>('monitoring/stats'),
      apiGet<BackendDistribution>('devices/distribution'),
      apiGet<BackendRecentAlarm[]>('alarms/recent', { limit: 10 }),
      apiGet<BackendQuickDevice[]>('devices/quick', { limit: 8 }),
    ])

    const bs = statsRes.result
    const bd = distRes.result
    const ba = alarmsRes.result || []
    const bqd = devicesRes.result || []

    const stats: DashboardStats = bs
      ? {
          totalDevices: bs.totalDevices,
          onlineDevices: bs.onlineDevices,
          offlineDevices: (bd?.offline ?? 0),
          activeAlarms: bs.activeAlarms,
          criticalAlarms: ba.filter(a => levelToNumber(a.level) >= 3).length,
        }
      : { totalDevices: 0, onlineDevices: 0, offlineDevices: 0, activeAlarms: 0, criticalAlarms: 0 }

    const deviceDistribution: DeviceStatusDistribution = bd
      ? { online: bd.online, offline: bd.offline, warning: 0, error: bd.error }
      : { online: 0, offline: 0, warning: 0, error: 0 }

    const recentAlarms: RecentAlarm[] = ba.map(a => ({
      id: a.id,
      deviceId: a.deviceId,
      deviceName: a.deviceName,
      alarmType: a.level,
      level: levelToNumber(a.level),
      message: a.message,
      timestamp: a.createdAt,
      acknowledged: a.status === 'acknowledged' || a.status === 'resolved',
    }))

    const quickDevices: QuickDevice[] = bqd.map(d => ({
      id: d.id,
      name: d.name,
      status: d.status,
      protocol: d.deviceType,
      lastSeen: d.lastSeen,
    }))

    return {
      result: { stats, deviceDistribution, recentAlarms, quickDevices },
      code: 0,
      msg: '',
    }
  },

  getStats: () =>
    apiGet<DashboardStats>('monitoring/stats'),

  getDeviceDistribution: () =>
    apiGet<DeviceStatusDistribution>('devices/distribution'),

  getRecentAlarms: (limit: number = 10) =>
    apiGet<RecentAlarm[]>('alarms/recent', { limit }),

  getSystemMetrics: () =>
    apiGet<DashboardMetrics>('monitoring/metrics'),

  getQuickDevices: (limit: number = 8) =>
    apiGet<QuickDevice[]>('devices/quick', { limit }),
}
