/**
 * 设备 API
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
    return apiGet<PaginatedResponse<Device>>('/devices', params as Record<string, any>);
  },

  async getDevice(id: string) {
    return apiGet<Device>(`/devices/${id}`);
  },

  async getDeviceProfile(id: string) {
    return apiGet<DeviceProfile>(`/devices/${id}/profile`);
  },

  async createDevice(data: CreateDeviceRequest) {
    return apiPost<Device>('/devices', data);
  },

  async updateDevice(id: string, data: Partial<CreateDeviceRequest>) {
    return apiPut<Device>(`/devices/${id}`, data);
  },

  async deleteDevice(id: string) {
    return apiDelete<void>(`/devices/${id}`);
  },

  async getDeviceCommands(deviceId: string) {
    return apiGet<DeviceCommand[]>(`/devices/${deviceId}/commands`);
  },

  async executeCommand(deviceId: string, commandName: string, params?: Record<string, any>) {
    return apiPost<any>(`/devices/${deviceId}/commands/${commandName}/execute`, params);
  },

  async getDeviceProperties(deviceId: string) {
    return apiGet<any[]>(`/devices/${deviceId}/properties`);
  },

  async updateDeviceProperty(deviceId: string, propertyName: string, value: any) {
    return apiPut<void>(`/devices/${deviceId}/properties/${propertyName}`, { value });
  },

  async createDeviceFromTemplate(data: { templateId: string; deviceInput: any }) {
    return apiPost<any>('/devices/from-template', data);
  },

  async exportDeviceAsTemplate(id: string) {
    return apiPost<{ templateId: string; name: string }>(`/devices/${id}/export-template`);
  },

  async cloneDevice(id: string) {
    return apiPost<Device>(`/devices/${id}/clone`);
  },
};
