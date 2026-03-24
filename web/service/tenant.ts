/**
 * 租户服务
 * 处理租户登录、注册和 API Key 管理
 */

import { apiGet, apiPost } from '@/lib/api-client'

// 租户类型定义
export interface Tenant {
  id: string
  name: string
  slug: string
  status: string
  plan_id: string
  subscription_status: string
}

// API Key 类型定义
export interface ApiKey {
  id: string
  name: string
  prefix: string
  permissions: string
  is_enabled: boolean
  request_count: number
  created_at: string
}

// 登录请求
export interface TenantLoginRequest {
  email: string
  password: string
}

// 注册请求
export interface TenantRegisterRequest {
  name: string
  slug: string
  email: string
  password: string
}

// API 响应类型
export interface TenantLoginResponse {
  token: string
  tenant: Tenant
}

export interface TenantRegisterResponse {
  token: string
  tenant: Tenant
}

// API 调用函数
export const tenantApi = {
  // 租户登录
  login: (data: TenantLoginRequest) =>
    apiPost<TenantLoginResponse>('/api/v1/tenants/login', data),

  // 租户注册
  register: (data: TenantRegisterRequest) =>
    apiPost<TenantRegisterResponse>('/api/v1/tenants/register', data),

  // 获取 API Keys
  getApiKeys: (tenantId: string) =>
    apiGet<ApiKey[]>(`/api/v1/tenants/${tenantId}/api-keys`),

  // 创建 API Key
  createApiKey: (tenantId: string, data: { name: string; permissions: string[] }) =>
    apiPost<{ raw_key: string; key: ApiKey }>(`/api/v1/tenants/${tenantId}/api-keys`, data),
}

// 存储键名
const TENANT_TOKEN_KEY = 'tenant_token'
const TENANT_DATA_KEY = 'tenant'

// 获取存储的租户 token
export const getTenantToken = (): string | null => {
  if (typeof window === 'undefined') return null
  return sessionStorage.getItem(TENANT_TOKEN_KEY) || localStorage.getItem(TENANT_TOKEN_KEY)
}

// 获取存储的租户数据
export const getStoredTenant = (): Tenant | null => {
  if (typeof window === 'undefined') return null
  const tenantData = sessionStorage.getItem(TENANT_DATA_KEY) || localStorage.getItem(TENANT_DATA_KEY)
  if (!tenantData) return null
  try {
    return JSON.parse(tenantData)
  } catch {
    return null
  }
}

// 保存租户 token（使用 sessionStorage，更安全）
export const saveTenantToken = (token: string): void => {
  if (typeof window === 'undefined') return
  sessionStorage.setItem(TENANT_TOKEN_KEY, token)
}

// 保存租户数据
export const saveTenantData = (tenant: Tenant): void => {
  if (typeof window === 'undefined') return
  sessionStorage.setItem(TENANT_DATA_KEY, JSON.stringify(tenant))
}

// 清除租户数据
export const clearTenantData = (): void => {
  if (typeof window === 'undefined') return
  sessionStorage.removeItem(TENANT_TOKEN_KEY)
  sessionStorage.removeItem(TENANT_DATA_KEY)
  localStorage.removeItem(TENANT_TOKEN_KEY)
  localStorage.removeItem(TENANT_DATA_KEY)
}
