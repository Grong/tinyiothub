/**
 * 系统相关类型定义
 * 前端统一使用 camelCase 命名
 */

export interface SystemConfig {
  id: string
  key: string
  value: string
  description?: string
  category?: string
  updatedAt: string
}

export interface SystemTask {
  id: string
  name: string
  type: string
  status: 'pending' | 'running' | 'completed' | 'failed'
  progress?: number
  result?: any
  createdAt: string
  updatedAt: string
}

export interface SystemHealth {
  status: 'healthy' | 'degraded' | 'unhealthy'
  checks: {
    [key: string]: {
      status: 'healthy' | 'degraded' | 'unhealthy'
      message?: string
      lastChecked: string
    }
  }
  timestamp: string
}

export interface SystemMetrics {
  timestamp: string
  cpuUsage: number
  memoryUsage: number
  diskUsage: number
  networkIn: number
  networkOut: number
  activeConnections: number
  uptime: number
}

export interface SystemFeatures {
  version?: string
  buildDate?: string
  environment?: string
  features?: {
    [key: string]: boolean
  }
}

export interface ComponentHealth {
  name: string
  status: 'healthy' | 'degraded' | 'unhealthy'
  message?: string
  lastChecked: string
}

export interface HealthStatus {
  status: 'healthy' | 'degraded' | 'unhealthy'
  components: ComponentHealth[]
  timestamp: string
}