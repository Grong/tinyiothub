/**
 * 仪表盘 API
 */

import { apiGet } from './client.js';
import type { DashboardData } from '../types/index.js';

export const dashboardApi = {
  async getDashboardData() {
    return apiGet<DashboardData>('/dashboard');
  },

  async getStats() {
    return apiGet<any>('/dashboard/stats');
  },

  async getDeviceDistribution() {
    return apiGet<any>('/dashboard/device-distribution');
  },

  async getSystemMetrics() {
    return apiGet<any>('/dashboard/system-metrics');
  },
};
