/**
 * Driver Health Dashboard API
 */

import { apiGet } from './client.js';

export interface DriverHealthInfo {
  driverName: string;
  version: string;
  loadedAt: string;
  refCount: number;
  status: 'active' | 'idle' | 'error' | 'unloading';
}

export interface WorkspaceDriverHealth {
  workspaceId: string;
  drivers: DriverHealthInfo[];
}

export const driverHealthApi = {
  async getWorkspaceHealth() {
    return apiGet<WorkspaceDriverHealth>('/driver-health/drivers');
  },
};
