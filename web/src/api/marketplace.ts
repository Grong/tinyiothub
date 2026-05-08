/**
 * Marketplace API — proxy to external marketplace + local publish
 */

import { apiGet, apiPost } from './client.js';

export interface MarketplaceTemplate {
  id: string;
  name: string;
  version: string;
  description?: string;
  category?: string;
  author?: string;
  tags?: string[];
  deviceType?: string;
  protocolType?: string;
  driverName?: string;
  rating?: number;
  downloadCount?: number;
}

export interface MarketplaceDriver {
  id: string;
  name: string;
  version: string;
  description?: string;
  protocolType?: string;
  rating?: number;
  downloadCount?: number;
}

export const marketplaceApi = {
  async getTemplates(params?: { category?: string; search?: string; page?: number; pageSize?: number }) {
    return apiGet<{ data: MarketplaceTemplate[]; pagination: { page: number; pageSize: number; totalPages: number; totalCount: number } }>('/marketplace/templates', params as Record<string, any>);
  },

  async getTemplate(id: string) {
    return apiGet<MarketplaceTemplate>(`/marketplace/templates/${id}`);
  },

  async installTemplate(id: string, version?: string) {
    return apiPost<string>(`/marketplace/templates/${id}/install`, { version });
  },

  async getDrivers(params?: { protocolType?: string; search?: string; page?: number; pageSize?: number }) {
    return apiGet<{ data: MarketplaceDriver[]; pagination: { page: number; pageSize: number; totalPages: number; totalCount: number } }>('/marketplace/drivers', params as Record<string, any>);
  },

  async getDriver(id: string) {
    return apiGet<MarketplaceDriver>(`/marketplace/drivers/${id}`);
  },

  async installDriver(id: string, version?: string) {
    return apiPost<string>(`/marketplace/drivers/${id}/install`, { version });
  },

  async publishTemplate(templateId: string) {
    return apiPost<Record<string, unknown>>('/marketplace/publish/template', { templateId });
  },
};
