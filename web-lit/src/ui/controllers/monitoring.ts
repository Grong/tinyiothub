// web-lit/src/ui/controllers/monitoring.ts
import type { AppViewState } from '../app-view-state'
import { apiGet } from '../api-client'

export async function loadMonitoringData(host: AppViewState): Promise<void> {
  host.monitoringLoading = true
  try {
    const res = await apiGet('monitoring/metrics')
    // Store in dashboard for now — monitoring reuses dashboard metrics
    if (res.result && host.dashboardData) {
      host.dashboardData.systemMetrics = res.result as typeof host.dashboardData.systemMetrics
    }
  } finally {
    host.monitoringLoading = false
  }
}
