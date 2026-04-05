/**
 * 告警管理服务 - Pure async API functions
 */

import { apiGet, apiPost, apiPut, apiDelete } from '../lib/api-client'
import type { PaginatedResponse } from '../lib/api-client'

// Types
export interface Alarm {
  id: string
  deviceId: string
  alarmType: string
  level: number
  message: string
  acknowledged: boolean
  resolved: boolean
  timestamp: string
}

export interface AlarmRule {
  id: string
  name: string
  deviceId?: string
  condition: string
  enabled: boolean
  // ... other rule fields
}

export interface AlarmStatistics {
  total: number
  critical: number
  warning: number
  info: number
  acknowledged: number
  unresolved: number
}

export interface AlarmQueryParams {
  deviceId?: string
  level?: number
  acknowledged?: boolean
  resolved?: boolean
  startTime?: string
  endTime?: string
  page?: number
  pageSize?: number
}

export interface StatisticsQueryParams {
  startTime?: string
  endTime?: string
  deviceId?: string
}

export interface CreateAlarmRuleRequest {
  name: string
  deviceId?: string
  condition: string
  level: number
  enabled: boolean
}

export interface UpdateAlarmRuleRequest {
  name?: string
  condition?: string
  level?: number
  enabled?: boolean
}

export interface AcknowledgeRequest {
  comment?: string
}

export interface ResolveRequest {
  comment?: string
}

export interface BatchAcknowledgeRequest {
  ids: string[]
  comment?: string
}

export interface BatchResolveRequest {
  ids: string[]
  comment?: string
}

export interface BatchOperationResult {
  success: number
  failed: number
  errors: string[]
}

// Pure async API functions
export const alarmApi = {
  getAlarms: (params?: AlarmQueryParams) =>
    apiGet<PaginatedResponse<Alarm>>('alarms', params),

  getAlarm: (id: string) =>
    apiGet<Alarm>(`alarms/${id}`),

  getAlarmStatistics: (params?: StatisticsQueryParams) =>
    apiGet<AlarmStatistics>('alarms/statistics', params),

  acknowledgeAlarm: (id: string, data?: AcknowledgeRequest) =>
    apiPost<void>(`alarms/${id}/acknowledge`, data),

  resolveAlarm: (id: string, data: ResolveRequest) =>
    apiPost<void>(`alarms/${id}/resolve`, data),

  batchAcknowledgeAlarms: (data: BatchAcknowledgeRequest) =>
    apiPost<BatchOperationResult>('alarms/batch-acknowledge', data),

  batchResolveAlarms: (data: BatchResolveRequest) =>
    apiPost<BatchOperationResult>('alarms/batch-resolve', data),

  getAlarmRules: (params?: { deviceId?: string }) =>
    apiGet<AlarmRule[]>('alarm-rules', params),

  getAlarmRule: (id: string) =>
    apiGet<AlarmRule>(`alarm-rules/${id}`),

  createAlarmRule: (data: CreateAlarmRuleRequest) =>
    apiPost<AlarmRule>('alarm-rules', data),

  updateAlarmRule: (id: string, data: UpdateAlarmRuleRequest) =>
    apiPut<AlarmRule>(`alarm-rules/${id}`, data),

  deleteAlarmRule: (id: string) =>
    apiDelete<void>(`alarm-rules/${id}`),

  toggleAlarmRule: (id: string, enabled: boolean) =>
    apiPost<void>(`alarm-rules/${id}/toggle`, { enabled }),
}
