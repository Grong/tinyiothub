/**
 * 告警 API
 */

import { apiGet, apiPost, apiPut, apiDelete } from './client.js';
import type {
  Alarm,
  AlarmRule,
  AlarmStatistics,
  AlarmQueryParams,
  CreateAlarmRuleRequest,
  UpdateAlarmRuleRequest,
  AcknowledgeRequest,
  ResolveRequest,
  BatchAcknowledgeRequest,
  BatchResolveRequest,
  BatchOperationResult,
} from '../types/index.js';
import type { PaginatedResponse } from './client.js';

export const alarmApi = {
  async getAlarms(params?: AlarmQueryParams) {
    return apiGet<PaginatedResponse<Alarm>>('/alarms', params as Record<string, any>);
  },

  async getAlarm(id: string) {
    return apiGet<Alarm>(`/alarms/${id}`);
  },

  async getStatistics(params?: { startTime?: string; endTime?: string }) {
    return apiGet<AlarmStatistics>('/alarms/statistics', params);
  },

  async acknowledgeAlarm(id: string, data?: AcknowledgeRequest) {
    return apiPut<void>(`/alarms/${id}/acknowledge`, data);
  },

  async resolveAlarm(id: string, data: ResolveRequest) {
    return apiPut<void>(`/alarms/${id}/resolve`, data);
  },

  async batchAcknowledge(data: BatchAcknowledgeRequest) {
    return apiPost<BatchOperationResult>('/alarms/batch/acknowledge', data);
  },

  async batchResolve(data: BatchResolveRequest) {
    return apiPost<BatchOperationResult>('/alarms/batch/resolve', data);
  },

  // Alarm Rules
  async getRules(params?: { page?: number; pageSize?: number; deviceId?: string }) {
    return apiGet<PaginatedResponse<AlarmRule>>('/alarm-rules', params as Record<string, any>);
  },

  async getRule(id: string) {
    return apiGet<AlarmRule>(`/alarm-rules/${id}`);
  },

  async createRule(data: CreateAlarmRuleRequest) {
    return apiPost<AlarmRule>('/alarm-rules', data);
  },

  async updateRule(id: string, data: UpdateAlarmRuleRequest) {
    return apiPut<AlarmRule>(`/alarm-rules/${id}`, data);
  },

  async deleteRule(id: string) {
    return apiDelete<void>(`/alarm-rules/${id}`);
  },

  async toggleRule(id: string, isEnabled: boolean) {
    return apiPut<void>(`/alarm-rules/${id}`, { isEnabled });
  },
};
