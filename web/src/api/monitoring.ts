/**
 * 监控 API
 */

import { apiGet } from './client.js';
import type { SystemMetrics, HealthStatus } from '../types/index.js';

export const monitoringApi = {
  async getSystemMetrics() {
    return apiGet<SystemMetrics>('/monitoring/metrics');
  },

  async getHealthStatus() {
    return apiGet<HealthStatus>('/monitoring/health');
  },

  async getDeviceMetrics(deviceId: string) {
    return apiGet<any>(`/monitoring/devices/${deviceId}`);
  },

  async getPerformanceData(params?: { startTime?: string; endTime?: string }) {
    return apiGet<any>('/monitoring/performance', params);
  },
};
