/**
 * API 调用接口
 * 
 * @deprecated 此文件已被重构，请使用新的 API 客户端：
 * - 统一客户端：@/lib/api-client
 * - 专门服务：@/service/* 文件
 */

import { apiGet as newApiGet, apiPost as newApiPost, apiPut as newApiPut, apiDelete as newApiDelete } from '@/lib/api-client'
import type { ApiResponse } from '@/types'

// 向后兼容的 API 调用方法
export const apiGet = <T>(endpoint: string, params?: Record<string, any>): Promise<ApiResponse<T>> => {
  console.warn('apiGet from @/lib/api is deprecated, use apiGet from @/lib/api-client or specific service hooks instead')
  return newApiGet<T>(endpoint, params)
}

export const apiPost = <T>(endpoint: string, data?: any): Promise<ApiResponse<T>> => {
  console.warn('apiPost from @/lib/api is deprecated, use apiPost from @/lib/api-client or specific service hooks instead')
  return newApiPost<T>(endpoint, data)
}

export const apiPut = <T>(endpoint: string, data?: any): Promise<ApiResponse<T>> => {
  console.warn('apiPut from @/lib/api is deprecated, use apiPut from @/lib/api-client or specific service hooks instead')
  return newApiPut<T>(endpoint, data)
}

export const apiDelete = <T>(endpoint: string): Promise<ApiResponse<T>> => {
  console.warn('apiDelete from @/lib/api is deprecated, use apiDelete from @/lib/api-client or specific service hooks instead')
  return newApiDelete<T>(endpoint)
}

export const apiPatch = <T>(endpoint: string, data?: any): Promise<ApiResponse<T>> => {
  console.warn('apiPatch from @/lib/api is deprecated, use apiPatch from @/lib/api-client or specific service hooks instead')
  return newApiPut<T>(endpoint, data) // 使用put作为patch的替代
}

// Token管理 - 由fetch服务内部处理
export const setAuthToken = (token: string | null) => {
  console.warn('setAuthToken from @/lib/api is deprecated, token management is now handled automatically')
  if (typeof window !== 'undefined') {
    if (token) {
      localStorage.setItem('auth-token', token)
    } else {
      localStorage.removeItem('auth-token')
    }
  }
}