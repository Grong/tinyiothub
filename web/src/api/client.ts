/**
 * IoT 统一 API 客户端
 * 响应格式: { code: number, msg: string, result: T }
 * 自动处理 snake_case ↔ camelCase 转换
 */

import { API_BASE } from './config.js';
import { keysToCamelCase, keysToSnakeCase, type KeysToCamelCase } from './case-converter.js';

// IoT API 统一响应格式
export interface ApiResponse<T = any> {
  code: number;
  msg: string;
  result: T | null;
}

// 分页响应（keysToCamelCase 会将 total_pages → totalPages, total_count → totalCount）
export interface PaginatedResponse<T> {
  data: T[];
  pagination: {
    page: number;
    pageSize: number;
    totalPages: number;
    totalCount: number;
  };
}

export class ApiError extends Error {
  constructor(
    public code: number,
    message: string,
    public data?: any,
  ) {
    super(message);
    this.name = 'ApiError';
  }
}

// HTTP 请求选项
interface RequestOptions {
  method?: 'GET' | 'POST' | 'PUT' | 'DELETE' | 'PATCH';
  headers?: Record<string, string>;
  body?: any;
  params?: Record<string, any>;
}

// 获取认证 token（供其他模块复用）
export const getAuthToken = (): string | null => {
  if (typeof window === 'undefined') return null;
  return sessionStorage.getItem('auth-token') || localStorage.getItem('auth-token');
};

// 获取 workspace id
export const getWorkspaceId = (): string | null => {
  if (typeof window === 'undefined') return null;
  return localStorage.getItem('workspace-id') || sessionStorage.getItem('workspace-id');
};

// 构建完整 URL（供其他模块复用）
export const buildUrl = (endpoint: string): string => {
  if (endpoint.startsWith('http://') || endpoint.startsWith('https://')) {
    return endpoint;
  }
  const normalizedEndpoint = endpoint.startsWith('/') ? endpoint : `/${endpoint}`;
  return `${API_BASE}${normalizedEndpoint}`;
};

// 底层 HTTP 请求
async function request<T>(endpoint: string, options: RequestOptions = {}): Promise<T> {
  const { method = 'GET', body, params, headers = {} } = options;

  const urlPath = buildUrl(endpoint);
  const url = urlPath.startsWith('http')
    ? new URL(urlPath)
    : new URL(urlPath, window.location.origin);

  if (params) {
    Object.entries(params).forEach(([key, value]) => {
      if (value !== undefined && value !== null) {
        url.searchParams.append(key, String(value));
      }
    });
  }

  const config: RequestInit = {
    method,
    credentials: 'include',
    headers: {
      'Content-Type': 'application/json',
      ...headers,
    },
  };

  const token = getAuthToken();
  if (token) {
    (config.headers as Record<string, string>)['Authorization'] = `Bearer ${token}`;
  }

  // 添加 workspace 上下文
  const wsId = getWorkspaceId();
  if (wsId) {
    (config.headers as Record<string, string>)['X-Workspace-Id'] = wsId;
  }

  if (body && method !== 'GET') {
    config.body = JSON.stringify(body);
  }

  const response = await fetch(url.toString(), config);

  if (response.status === 401) {
    sessionStorage.removeItem('auth-token');
    localStorage.removeItem('auth-token');
    window.dispatchEvent(new CustomEvent('auth-error', {
      detail: { message: '认证已过期' },
    }));
    throw new ApiError(401, 'Unauthorized - 请重新登录');
  }

  if (!response.ok) {
    let errorData: any = {};
    try {
      errorData = await response.json();
    } catch {
      // ignore
    }
    const errorMessage = errorData?.msg || errorData?.message || `HTTP ${response.status}`;
    throw new ApiError(response.status, errorMessage, errorData);
  }

  return await response.json();
}

// API 客户端 — 所有方法返回 ApiResponse，自动做 case 转换
export class ApiClient {
  static async get<T>(
    endpoint: string,
    params?: Record<string, any>,
  ): Promise<ApiResponse<KeysToCamelCase<T>>> {
    const snakeParams = params ? keysToSnakeCase(params) : undefined;
    const response = await request<ApiResponse<T>>(endpoint, {
      method: 'GET',
      params: snakeParams,
    });
    if (response.code !== 0) {
      throw new ApiError(response.code, response.msg || '请求失败', response);
    }
    return {
      ...response,
      result: response.result ? keysToCamelCase(response.result) : response.result,
    } as ApiResponse<KeysToCamelCase<T>>;
  }

  static async post<T>(
    endpoint: string,
    data?: any,
  ): Promise<ApiResponse<KeysToCamelCase<T>>> {
    const snakeData = data ? keysToSnakeCase(data) : undefined;
    const response = await request<ApiResponse<T>>(endpoint, {
      method: 'POST',
      body: snakeData,
    });
    if (response.code !== 0) {
      throw new ApiError(response.code, response.msg || '请求失败', response);
    }
    return {
      ...response,
      result: response.result ? keysToCamelCase(response.result) : response.result,
    } as ApiResponse<KeysToCamelCase<T>>;
  }

  static async put<T>(
    endpoint: string,
    data?: any,
  ): Promise<ApiResponse<KeysToCamelCase<T>>> {
    const snakeData = data ? keysToSnakeCase(data) : undefined;
    const response = await request<ApiResponse<T>>(endpoint, {
      method: 'PUT',
      body: snakeData,
    });
    if (response.code !== 0) {
      throw new ApiError(response.code, response.msg || '请求失败', response);
    }
    return {
      ...response,
      result: response.result ? keysToCamelCase(response.result) : response.result,
    } as ApiResponse<KeysToCamelCase<T>>;
  }

  static async delete<T>(
    endpoint: string,
  ): Promise<ApiResponse<KeysToCamelCase<T>>> {
    const response = await request<ApiResponse<T>>(endpoint, {
      method: 'DELETE',
    });
    if (response.code !== 0) {
      throw new ApiError(response.code, response.msg || '请求失败', response);
    }
    return {
      ...response,
      result: response.result ? keysToCamelCase(response.result) : response.result,
    } as ApiResponse<KeysToCamelCase<T>>;
  }

  static async patch<T>(
    endpoint: string,
    data?: any,
  ): Promise<ApiResponse<KeysToCamelCase<T>>> {
    const snakeData = data ? keysToSnakeCase(data) : undefined;
    const response = await request<ApiResponse<T>>(endpoint, {
      method: 'PATCH',
      body: snakeData,
    });
    if (response.code !== 0) {
      throw new ApiError(response.code, response.msg || '请求失败', response);
    }
    return {
      ...response,
      result: response.result ? keysToCamelCase(response.result) : response.result,
    } as ApiResponse<KeysToCamelCase<T>>;
  }

  static async upload<T>(
    endpoint: string,
    formData: FormData,
    onProgress?: (pct: number) => void,
  ): Promise<ApiResponse<KeysToCamelCase<T>>> {
    const url = buildUrl(endpoint);
    const token = getAuthToken();
    const wsId = getWorkspaceId();

    return new Promise((resolve, reject) => {
      const xhr = new XMLHttpRequest();
      xhr.open('POST', url);
      xhr.withCredentials = true;

      if (token) xhr.setRequestHeader('Authorization', `Bearer ${token}`);
      if (wsId) xhr.setRequestHeader('X-Workspace-Id', wsId);

      if (onProgress) {
        xhr.upload.addEventListener('progress', (e) => {
          if (e.lengthComputable) onProgress(Math.round((e.loaded / e.total) * 100));
        });
      }

      xhr.addEventListener('load', () => {
        try {
          const response = JSON.parse(xhr.responseText) as ApiResponse<T>;
          if (response.code !== 0) {
            reject(new ApiError(response.code, response.msg || '上传失败', response));
            return;
          }
          resolve({
            ...response,
            result: response.result ? keysToCamelCase(response.result) : response.result,
          } as ApiResponse<KeysToCamelCase<T>>);
        } catch {
          reject(new ApiError(xhr.status, `HTTP ${xhr.status}`));
        }
      });

      xhr.addEventListener('error', () => reject(new ApiError(0, '网络错误')));
      xhr.addEventListener('abort', () => reject(new ApiError(0, '上传已取消')));

      xhr.send(formData);
    });
  }
}

// 便捷导出
export const {
  get: apiGet,
  post: apiPost,
  put: apiPut,
  delete: apiDelete,
  patch: apiPatch,
  upload: apiUpload,
} = ApiClient;

// File-based Skill (matches SkillInfoDto from backend)
export interface Skill {
  name: string;         // skill file name (without .md)
  description: string;  // from YAML frontmatter description field
  content: string;      // body of the .md file (after frontmatter)
}

// Frontmatter fields stored in the skill file itself
export interface CreateSkillRequest {
  workspace_id: string;
  skill_name: string;
  skill_content: string;  // full markdown including YAML frontmatter
}

export interface UpdateSkillRequest {
  skill_content: string;  // full markdown including YAML frontmatter
}

// Skills (file-based, name is string identifier)
export async function listSkills(workspaceId?: string): Promise<ApiResponse<Skill[]>> {
  const params = new URLSearchParams();
  if (workspaceId) params.set("workspace_id", workspaceId);
  return apiGet(`/agents/skills?${params}`);
}

export async function getSkill(name: string, workspaceId?: string): Promise<ApiResponse<Skill>> {
  const params = new URLSearchParams();
  if (workspaceId) params.set("workspace_id", workspaceId);
  return apiGet(`/agents/skills/${encodeURIComponent(name)}?${params}`);
}

export async function createSkill(data: CreateSkillRequest): Promise<ApiResponse<Skill>> {
  return apiPost("/agents/skills", data);
}

export async function updateSkill(name: string, data: UpdateSkillRequest, workspaceId?: string): Promise<ApiResponse<Skill>> {
  const params = new URLSearchParams();
  if (workspaceId) params.set("workspace_id", workspaceId);
  return apiPut(`/agents/skills/${encodeURIComponent(name)}?${params}`, data);
}

export async function deleteSkill(name: string, workspaceId?: string): Promise<ApiResponse<void>> {
  const params = new URLSearchParams();
  if (workspaceId) params.set("workspace_id", workspaceId);
  return apiDelete(`/agents/skills/${encodeURIComponent(name)}?${params}`);
}
