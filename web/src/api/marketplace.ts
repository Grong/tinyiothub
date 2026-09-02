/**
 * Marketplace API — proxy to external marketplace + local publish
 */

import { ApiError, apiGet, apiPost, buildUrl, getAuthToken, getWorkspaceId, type ApiResponse } from './client.js';

export interface LocalizedString {
  zh?: string;
  en?: string;
}

export interface TemplateProperty {
  name: string;
  display_name?: string | LocalizedString;
  description?: string | LocalizedString;
  data_type: string;
  unit?: string;
  min_value?: number;
  max_value?: number;
  default_value?: string;
  is_read_only?: boolean;
  is_required?: boolean;
}

export interface TemplateCommand {
  name: string;
  display_name?: string | LocalizedString;
  description?: string | LocalizedString;
  parameters?: string;
  parameter_schema?: string;
  is_required?: boolean;
}

export interface TemplateDeviceInfo {
  default_name_pattern?: string;
  default_display_name_pattern?: string | LocalizedString;
  default_description?: string | LocalizedString;
  required_fields?: string[];
}

export interface MarketplaceTemplate {
  name: string;
  display_name?: string | LocalizedString;
  displayName?: string;
  version: string;
  description?: string | LocalizedString;
  category?: string;
  author?: string;
  tags?: string[];
  protocolType?: string;
  driverName?: string;
  rating?: number;
  downloadCount?: number;
  manufacturer?: string;
  properties?: TemplateProperty[];
  commands?: TemplateCommand[];
  device_info?: TemplateDeviceInfo;
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

// ──────────────────────────────────────────────
// Scene pack (composition thing_template) APIs
// ──────────────────────────────────────────────

export interface SceneParameter {
  name: string;
  type: "int";
  default: number;
  min: number;
  max: number;
  /** apiGet 会把 display_name camelize 成 displayName；两者都容忍 */
  displayName?: LocalizedString;
  display_name?: LocalizedString;
}

export interface ThingTemplateItem {
  id: string;
  name: string;
  displayName?: string;
  description?: string;
  category: string;
  isBuiltin: boolean;
  isComposition: boolean;
  parameterCount: number;
}

export interface ThingTemplateListResult {
  data: ThingTemplateItem[];
  pagination: { page: number; pageSize: number; totalPages: number; totalCount: number };
}

export interface SceneTemplateDetail extends ThingTemplateItem {
  parameters: SceneParameter[];
  structureSummary: { parameterCount: number; maxDepth: number };
}

export interface InstantiateResult {
  nodeCount: number;
  rootThingId: string | null;
  treePreview: string;
  warnings: string[];
}

export interface InstantiateBody {
  sceneName: string;
  parentId?: string;
  parameterValues?: Record<string, number>;
  dryRun?: boolean;
}

// instantiate 后端 serde 是 camelCase（rename_all = "camelCase"），
// 不能走 apiPost（会把 body 转 snake_case 导致 422）——原样发送 + 手动解包统一响应。
async function postEnvelopeRaw<T>(endpoint: string, body: unknown): Promise<T> {
  const headers: Record<string, string> = { 'Content-Type': 'application/json' };
  const token = getAuthToken();
  if (token) headers['Authorization'] = `Bearer ${token}`;
  const wsId = getWorkspaceId();
  if (wsId) headers['X-Workspace-Id'] = wsId;

  const response = await fetch(buildUrl(endpoint), {
    method: 'POST',
    credentials: 'include',
    headers,
    body: JSON.stringify(body),
  });
  const data = (await response.json().catch(() => ({}))) as ApiResponse<T>;
  if (!response.ok || data.code !== 0) {
    throw new ApiError(data.code ?? response.status, data.msg || `HTTP ${response.status}`, data);
  }
  return data.result as T;
}

export const sceneApi = {
  async listThingTemplates(composition?: boolean) {
    const res = await apiGet<ThingTemplateListResult>(
      '/marketplace/thing-templates',
      composition === undefined ? undefined : { composition },
    );
    return res.result;
  },

  async getThingTemplate(id: string) {
    const res = await apiGet<SceneTemplateDetail>(`/marketplace/thing-templates/${encodeURIComponent(id)}`);
    return res.result;
  },

  instantiate(id: string, body: InstantiateBody) {
    return postEnvelopeRaw<InstantiateResult>(
      `/marketplace/thing-templates/${encodeURIComponent(id)}/instantiate`,
      body,
    );
  },

  // 后端返回 raw JSON attachment（非统一响应包装），直接取 blob + 文件名。
  async exportAsTemplate(thingId: string): Promise<{ blob: Blob; filename: string }> {
    const headers: Record<string, string> = {};
    const token = getAuthToken();
    if (token) headers['Authorization'] = `Bearer ${token}`;
    const wsId = getWorkspaceId();
    if (wsId) headers['X-Workspace-Id'] = wsId;

    const response = await fetch(buildUrl(`/things/${encodeURIComponent(thingId)}/export-as-template`), {
      method: 'POST',
      credentials: 'include',
      headers,
    });
    if (!response.ok) {
      let msg = `HTTP ${response.status}`;
      try {
        const data = await response.json();
        msg = data?.msg || data?.message || msg;
      } catch {
        // ignore
      }
      throw new ApiError(response.status, msg);
    }
    const disposition = response.headers.get('Content-Disposition') ?? '';
    const match = /filename="?([^";]+)"?/.exec(disposition);
    const filename = match?.[1] ?? `scene-template-${thingId}.json`;
    const blob = await response.blob();
    return { blob, filename };
  },
};
