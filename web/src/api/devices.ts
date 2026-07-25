/**
 * 物 API — 底层调用 /api/v1/things 路由
 */

import { apiGet, apiPost, apiPut, apiDelete } from './client.js';
import type {
  Device,
  DeviceListParams,
  DeviceProfile,
  CreateDeviceRequest,
  DeviceCommand,
} from '../types/index.js';
import type { PaginatedResponse } from './client.js';

export const deviceApi = {
  async getDevices(params?: DeviceListParams) {
    return apiGet<PaginatedResponse<Device>>('/things', params as Record<string, any>);
  },

  async getDevice(id: string) {
    return apiGet<Device>(`/things/${id}`);
  },

  async getDeviceProfile(id: string) {
    return apiGet<DeviceProfile>(`/things/${id}/profile`);
  },

  async createDevice(data: CreateDeviceRequest) {
    return apiPost<Device>('/things', data);
  },

  async updateDevice(id: string, data: Partial<CreateDeviceRequest>) {
    return apiPut<Device>(`/things/${id}`, data);
  },

  async deleteDevice(id: string) {
    return apiDelete<void>(`/things/${id}`);
  },

  async getDeviceCommands(deviceId: string) {
    // Thing actions endpoint — commands are now actions on things
    return apiGet<DeviceCommand[]>(`/things/${deviceId}/commands`);
  },

  async executeCommand(deviceId: string, commandName: string, params?: Record<string, any>) {
    // invoke action via thing confirm endpoint
    return apiPost<any>(`/things/${deviceId}/actions/${commandName}/confirm`, { token: "direct", params });
  },

  async getDeviceProperties(deviceId: string) {
    // Properties are included in the profile response
    return apiGet<any[]>(`/things/${deviceId}/profile`);
  },

  async updateDeviceProperty(deviceId: string, propertyName: string, value: any) {
    return apiPut<void>(`/things/${deviceId}/properties/${propertyName}`, { value });
  },

  async createDeviceFromTemplate(data: { templateId: string; deviceInput: any }) {
    // Map to thing create with template
    return apiPost<any>('/things', {
      name: data.deviceInput?.name,
      templateId: data.templateId,
      ...data.deviceInput,
    });
  },

  async exportDeviceAsTemplate(id: string) {
    // DTDL export endpoint as fallback
    return apiPost<{ templateId: string; name: string }>(`/things/templates/${id}/export/dtdl`);
  },

  async cloneDevice(id: string) {
    // Get thing then create a copy
    const thing = await apiGet<any>(`/things/${id}`);
    const cloneData = { ...thing.result, name: `${thing.result?.name || 'clone'} (副本)`, id: undefined };
    return apiPost<any>('/things', cloneData);
  },
};
