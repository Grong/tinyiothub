/**
 * 事件 API
 */

import { apiGet } from './client.js';
import type { DeviceEvent } from '../types/index.js';
import type { PaginatedResponse } from './client.js';

export const eventApi = {
  async getEvents(params?: {
    page?: number;
    pageSize?: number;
    deviceId?: string;
    eventType?: string;
    level?: string;
    startTime?: string;
    endTime?: string;
  }) {
    return apiGet<PaginatedResponse<DeviceEvent>>('/events', params as Record<string, any>);
  },

  async getEvent(id: string) {
    return apiGet<DeviceEvent>(`/events/${id}`);
  },
};
