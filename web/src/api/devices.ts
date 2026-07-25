/**
 * 物 API 适配器 — 委托到 /api/v1/things 路由
 *
 * 保留 deviceApi 命名是为了兼容现有 `<view-devices>` 组件。
 * 底层全部走 thing API，不做路由重定向。
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
    return apiGet<DeviceCommand[]>(`/things/${deviceId}/commands`);
  },

  async executeCommand(deviceId: string, commandName: string, params?: Record<string, any>) {
    return apiPost<any>(`/things/${deviceId}/commands/${commandName}/execute`, params);
  },

  async getDeviceProperties(deviceId: string) {
    return apiGet<any[]>(`/things/${deviceId}/properties`);
  },

  async updateDeviceProperty(deviceId: string, propertyName: string, value: any) {
    return apiPut<void>(`/things/${deviceId}/properties/${propertyName}`, { value });
  },

  async createDeviceFromTemplate(data: { templateId: string; deviceInput: any }) {
    return apiPost<any>('/things', { ...data.deviceInput, templateId: data.templateId });
  },

  async exportDeviceAsTemplate(id: string) {
    return apiPost<{ templateId: string; name: string }>(`/things/${id}/export-template`);
  },

  async cloneDevice(id: string) {
    return apiPost<Device>(`/things/${id}/clone`);
  },
};
