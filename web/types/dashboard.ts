/**
 * Dashboard 相关类型定义
 */

export interface DashboardStats {
  totalDevices: number
  onlineDevices: number
  activeAlarms: number
  systemStatus: 'healthy' | 'warning' | 'error'
  systemUptime: number
  todayMessages: number
  monthlyGrowth: {
    devices: number
    messages: number
  }
}

export interface DeviceStatusDistribution {
  online: number
  offline: number
  error: number
  maintenance: number
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
  level: 'info' | 'warning' | 'error' | 'critical'
  message: string
  createdAt: string
  status: 'active' | 'acknowledged' | 'resolved'
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
  status: 'online' | 'offline' | 'error' | 'maintenance'
  lastSeen: string
  type: string
}

export interface DashboardData {
  stats: DashboardStats
  deviceDistribution: DeviceStatusDistribution
  dataTrends: DataTrend[]
  protocolUsage: ProtocolUsage[]
  recentAlarms: RecentAlarm[]
  systemMetrics: DashboardMetrics
  quickDevices: QuickDevice[]
}