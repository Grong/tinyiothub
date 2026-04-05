/**
 * 统一的 API 客户端
 * 基于 TanStack Query 的最佳实践
 * 自动处理 snake_case 和 camelCase 转换
 */

import { keysToCamelCase, keysToSnakeCase, type KeysToCamelCase } from './case-converter'
import { API_PREFIX } from './config'
import { $currentWorkspaceId } from '../stores/workspace-store'

// 统一的 API 响应类型
export interface ApiResponse<T = unknown> {
  code: number
  msg: string
  result: T | null
}

export interface PaginatedResponse<T> {
  data: T[]
  pagination: {
    page: number
    pageSize: number
    totalPages: number
    totalCount: number
  }
}

// 自定义 API 错误类
export class ApiError extends Error {
  constructor(
    message: string,
    public code: number,
    public data: unknown,
    public status: number
  ) {
    super(message)
    this.name = 'ApiError'
  }
}

// HTTP 请求选项
interface RequestOptions {
  method?: 'GET' | 'POST' | 'PUT' | 'DELETE' | 'PATCH'
  headers?: Record<string, string>
  body?: any
  params?: Record<string, any>
}

// 获取认证token - 使用 sessionStorage 替代 localStorage 以减少 XSS 持久化风险
// 注意: sessionStorage 在标签页关闭时自动清除，比 localStorage 更安全
const getAuthToken = (): string | null => {
  if (typeof window === 'undefined') return null
  return sessionStorage.getItem('auth-token')
}

// 构建完整URL
const buildUrl = (endpoint: string): string => {
  // 如果是完整的 http/https URL，直接返回
  if (endpoint.startsWith('http://') || endpoint.startsWith('https://')) {
    return endpoint
  }
  const normalizedEndpoint = endpoint.startsWith('/') ? endpoint : `/${endpoint}`
  return `${API_PREFIX}${normalizedEndpoint}`
}

// 刷新token mutex - 防止多个并发请求同时刷新
let refreshPromise: Promise<boolean> | null = null

const refreshToken = async (): Promise<boolean> => {
  if (refreshPromise) return refreshPromise
  refreshPromise = (async () => {
    try {
      const response = await fetch(buildUrl('auth/refresh'), {
        method: 'POST',
        credentials: 'include',
      })
      if (response.ok) {
        const data = await response.json()
        if (data.code === 0 && data.result?.access_token) {
          sessionStorage.setItem('auth-token', data.result.access_token)
          return true
        }
      }
    } catch {
      console.warn('[api-client] Token refresh failed')
    }
    return false
  })()
  try {
    return await refreshPromise
  } finally {
    refreshPromise = null
  }
}

// 清除认证状态
const clearAuth = () => {
  sessionStorage.removeItem('auth-token')
  if (typeof window !== 'undefined') {
    window.dispatchEvent(new CustomEvent('auth-error', {
      detail: { message: 'Authentication expired' }
    }))
  }
}

// 底层HTTP请求函数
async function request<T>(endpoint: string, options: RequestOptions = {}): Promise<T> {
  const { method = 'GET', body, params, headers = {} } = options

  // 构建完整URL - 处理相对路径和绝对路径
  const urlPath = buildUrl(endpoint)
  const url = urlPath.startsWith('http')
    ? new URL(urlPath)
    : new URL(urlPath, typeof window !== 'undefined' ? window.location.origin : 'http://localhost:3000')

  // 添加查询参数
  if (params) {
    Object.entries(params).forEach(([key, value]) => {
      if (value !== undefined && value !== null) {
        url.searchParams.append(key, String(value))
      }
    })
  }

  // 构建请求配置
  const config: RequestInit = {
    method,
    credentials: 'include',
    headers: {
      'Content-Type': 'application/json',
      ...headers,
    },
  }

  // 添加认证token
  const token = getAuthToken()
  if (token) {
    config.headers = {
      ...config.headers,
      'Authorization': `Bearer ${token}`,
    }
  }

  // 添加 workspace 上下文（可选，未选择时显示租户全部数据）
  const workspaceId = $currentWorkspaceId.get()
  if (workspaceId) {
    config.headers = {
      ...config.headers,
      'X-Workspace-Id': workspaceId,
    }
  }

  // 添加请求体
  if (body && method !== 'GET') {
    config.body = JSON.stringify(body)
  }

  const response = await fetch(url.toString(), config)

  // 处理401未授权错误 - 尝试刷新token
  if (response.status === 401) {
    const refreshed = await refreshToken()
    if (refreshed) {
      const newToken = sessionStorage.getItem('auth-token')
      config.headers = {
        ...config.headers,
        'Authorization': `Bearer ${newToken}`,
      }
      const retryResponse = await fetch(url.toString(), config)
      if (retryResponse.ok) {
        return await retryResponse.json()
      }
      // Refresh succeeded but retry failed — auth is broken
      clearAuth()
      throw new Error('Unauthorized - please login again')
    } else {
      clearAuth()
      throw new Error('Unauthorized - please login again')
    }
  }

  // 处理其他HTTP错误
  if (!response.ok) {
    let errorData: any = {}
    try {
      errorData = await response.json()
    } catch {
      // JSON解析失败时使用默认错误信息
    }

    const errorMessage = errorData?.msg || errorData?.message || `HTTP ${response.status}`
    throw new ApiError(errorMessage, errorData?.code ?? -1, errorData, response.status)
  }

  return await response.json()
}

// API 客户端类
export class ApiClient {
  /**
   * GET 请求
   */
  static async get<T>(
    endpoint: string,
    params?: Record<string, any>
  ): Promise<ApiResponse<KeysToCamelCase<T>>> {
    // 将参数转换为 snake_case
    const snakeCaseParams = params ? keysToSnakeCase(params) : undefined

    const response = await request<ApiResponse<T>>(endpoint, {
      method: 'GET',
      params: snakeCaseParams
    })

    // 检查API响应中的code字段
    if (response.code !== 0) {
      throw new ApiError(response.msg || 'Request failed', response.code, response, 0)
    }

    // 将响应转换为 camelCase
    return {
      ...response,
      result: response.result ? keysToCamelCase(response.result) : response.result
    } as ApiResponse<KeysToCamelCase<T>>
  }

  /**
   * POST 请求
   */
  static async post<T>(
    endpoint: string,
    data?: any
  ): Promise<ApiResponse<KeysToCamelCase<T>>> {
    // 将请求数据转换为 snake_case
    const snakeCaseData = data ? keysToSnakeCase(data) : undefined

    const response = await request<ApiResponse<T>>(endpoint, {
      method: 'POST',
      body: snakeCaseData
    })

    // 检查API响应中的code字段
    if (response.code !== 0) {
      throw new ApiError(response.msg || 'Request failed', response.code, response, 0)
    }

    // 将响应转换为 camelCase
    return {
      ...response,
      result: response.result ? keysToCamelCase(response.result) : response.result
    } as ApiResponse<KeysToCamelCase<T>>
  }

  /**
   * PUT 请求
   */
  static async put<T>(
    endpoint: string,
    data?: any
  ): Promise<ApiResponse<KeysToCamelCase<T>>> {
    // 将请求数据转换为 snake_case
    const snakeCaseData = data ? keysToSnakeCase(data) : undefined

    const response = await request<ApiResponse<T>>(endpoint, {
      method: 'PUT',
      body: snakeCaseData
    })

    // 检查API响应中的code字段
    if (response.code !== 0) {
      throw new ApiError(response.msg || 'Request failed', response.code, response, 0)
    }

    // 将响应转换为 camelCase
    return {
      ...response,
      result: response.result ? keysToCamelCase(response.result) : response.result
    } as ApiResponse<KeysToCamelCase<T>>
  }

  /**
   * DELETE 请求
   */
  static async delete<T>(
    endpoint: string
  ): Promise<ApiResponse<KeysToCamelCase<T>>> {
    const response = await request<ApiResponse<T>>(endpoint, {
      method: 'DELETE'
    })

    // 检查API响应中的code字段
    if (response.code !== 0) {
      throw new ApiError(response.msg || 'Request failed', response.code, response, 0)
    }

    // 将响应转换为 camelCase
    return {
      ...response,
      result: response.result ? keysToCamelCase(response.result) : response.result
    } as ApiResponse<KeysToCamelCase<T>>
  }

  /**
   * PATCH 请求
   */
  static async patch<T>(
    endpoint: string,
    data?: any
  ): Promise<ApiResponse<KeysToCamelCase<T>>> {
    // 将请求数据转换为 snake_case
    const snakeCaseData = data ? keysToSnakeCase(data) : undefined

    const response = await request<ApiResponse<T>>(endpoint, {
      method: 'PATCH',
      body: snakeCaseData
    })

    // 检查API响应中的code字段
    if (response.code !== 0) {
      throw new ApiError(response.msg || 'Request failed', response.code, response, 0)
    }

    // 将响应转换为 camelCase
    return {
      ...response,
      result: response.result ? keysToCamelCase(response.result) : response.result
    } as ApiResponse<KeysToCamelCase<T>>
  }
}

// 导出便捷方法
export const apiGet = ApiClient.get
export const apiPost = ApiClient.post
export const apiPut = ApiClient.put
export const apiDelete = ApiClient.delete
export const apiPatch = ApiClient.patch
