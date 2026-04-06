// web-lit/src/ui/controllers/dashboard.ts
import type { AppViewState } from '../app-view-state'
import type { DashboardData } from '../types'
import { apiGet } from '../api-client'

export async function loadDashboard(host: AppViewState): Promise<void> {
  host.dashboardLoading = true
  try {
    const res = await apiGet<DashboardData>('dashboard')
    if (res.result) {
      host.dashboardData = res.result
    }
  } catch {
    // Fallback: compose from individual endpoints like the old service did
    try {
      const [statsRes, distRes, alarmsRes, quickRes] = await Promise.all([
        apiGet('monitoring/stats'),
        apiGet('devices/distribution'),
        apiGet('alarms/recent', { limit: 10 }),
        apiGet('devices/quick', { limit: 8 }),
      ])
      host.dashboardData = {
        stats: statsRes.result as DashboardData['stats'],
        deviceDistribution: distRes.result as DashboardData['deviceDistribution'],
        dataTrends: [],
        protocolUsage: [],
        recentAlarms: (alarmsRes.result as DashboardData['recentAlarms']) || [],
        systemMetrics: { cpu: 0, memory: 0, disk: 0, network: { inbound: 0, outbound: 0 } },
        quickDevices: (quickRes.result as DashboardData['quickDevices']) || [],
      }
    } catch {
      host.dashboardData = null
    }
  } finally {
    host.dashboardLoading = false
  }
}
