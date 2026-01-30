/**
 * 统一的 API 客户端
 * 基于 TanStack Query 的最佳实践
 * 自动处理 snake_case 和 camelCase 转换
 */

import { keysToCamelCase, keysToSnakeCase, type KeysToCamelCase } from '@/lib/case-converter'
import type { ApiResponse } from '@/types'
import { API_PREFIX } from '@/config'

// 统一的 API 响应类型
export interface PaginatedResponse<T> {
  data: T[]
  pagination: {
    page: number
    pageSize: number
    totalPages: number
    totalCount: number
  }
}

// HTTP 请求选项
interface RequestOptions {
  method?: 'GET' | 'POST' | 'PUT' | 'DELETE' | 'PATCH'
  headers?: Record<string, string>
  body?: any
  params?: Record<string, any>
}

// 获取认证token
const getAuthToken = (): string | null => {
  if (typeof window === 'undefined') return null
  return localStorage.getItem('auth-token')
}

// 构建完整URL
const buildUrl = (endpoint: string): string => {
  const normalizedEndpoint = endpoint.startsWith('/') ? endpoint : `/${endpoint}`
  return `${API_PREFIX}${normalizedEndpoint}`
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
  
  // 添加请求体
  if (body && method !== 'GET') {
    config.body = JSON.stringify(body)
  }
  
  try {
    const response = await fetch(url.toString(), config)
    
    // 处理401未授权错误
    if (response.status === 401) {
      if (typeof window !== 'undefined') {
        localStorage.removeItem('auth-token')
        const event = new CustomEvent('auth-error', { 
          detail: { message: 'Authentication expired' }
        })
        window.dispatchEvent(event)
      }
      throw new Error('Unauthorized - please login again')
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
      const error = new Error(errorMessage)
      ;(error as any).data = errorData
      ;(error as any).status = response.status
      
      throw error
    }
    
    return await response.json()
  } catch (error) {
    // 重新抛出错误，保持错误信息
    throw error
  }
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
      const error = new Error(response.msg || '请求失败')
      ;(error as any).data = response
      ;(error as any).code = response.code
      throw error
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
      const error = new Error(response.msg || '请求失败')
      ;(error as any).data = response
      ;(error as any).code = response.code
      throw error
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
      const error = new Error(response.msg || '请求失败')
      ;(error as any).data = response
      ;(error as any).code = response.code
      throw error
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
      const error = new Error(response.msg || '请求失败')
      ;(error as any).data = response
      ;(error as any).code = response.code
      throw error
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
      const error = new Error(response.msg || '请求失败')
      ;(error as any).data = response
      ;(error as any).code = response.code
      throw error
    }
    
    // 将响应转换为 camelCase
    return {
      ...response,
      result: response.result ? keysToCamelCase(response.result) : response.result
    } as ApiResponse<KeysToCamelCase<T>>
  }
}

// 导出便捷方法
export const { get: apiGet, post: apiPost, put: apiPut, delete: apiDelete, patch: apiPatch } = ApiClient