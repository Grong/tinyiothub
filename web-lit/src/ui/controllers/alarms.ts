// web-lit/src/ui/controllers/alarms.ts
import type { AppViewState } from '../app-view-state'
import type { Alarm, AlarmQueryParams } from '../types'
import { apiGet, apiPost } from '../api-client'

interface AlarmListResponse {
  items: Alarm[]
  total: number
  page: number
  pageSize: number
  totalPages: number
}

export async function loadAlarms(host: AppViewState, params?: AlarmQueryParams): Promise<void> {
  host.alarmsLoading = true
  try {
    const mergedParams = { ...host.alarmQueryParams, ...params }
    host.alarmQueryParams = mergedParams
    const res = await apiGet<AlarmListResponse>('alarms', mergedParams as Record<string, unknown>)
    if (res.result) {
      host.alarms = res.result.items
      host.alarmsPage = res.result.page
      host.alarmsTotalPages = res.result.totalPages
      host.alarmCount = res.result.items.filter(a => a.status === 'Active').length
    }
  } finally {
    host.alarmsLoading = false
  }
}

export async function acknowledgeAlarm(host: AppViewState, alarmId: string): Promise<void> {
  await apiPost(`alarms/${alarmId}/acknowledge`)
  host.alarms = host.alarms.map(a =>
    a.id === alarmId ? { ...a, status: 'Acknowledged' as const, isAcknowledged: true } : a
  )
}

export async function resolveAlarm(host: AppViewState, alarmId: string): Promise<void> {
  await apiPost(`alarms/${alarmId}/resolve`)
  host.alarms = host.alarms.map(a =>
    a.id === alarmId ? { ...a, status: 'Resolved' as const, isResolved: true } : a
  )
}
