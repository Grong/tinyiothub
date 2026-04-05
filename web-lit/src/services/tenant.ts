/**
 * 租户服务 - Pure async API functions
 */

import { apiGet, apiPost } from '../lib/api-client'

// Types
export interface Tenant {
  id: string
  name: string
  slug: string
  status: string
  plan_id: string
  subscription_status: string
}

export interface ApiKey {
  id: string
  name: string
  prefix: string
  permissions: string
  is_enabled: boolean
  request_count: number
  created_at: string
}

export interface TenantLoginRequest {
  email: string
  password: string
}

export interface TenantRegisterRequest {
  name: string
  slug: string
  email: string
  password: string
}

export interface TenantLoginResponse {
  token: string
  tenant: Tenant
}

export interface TenantRegisterResponse {
  token: string
  tenant: Tenant
}

// Pure async API functions
export const tenantApi = {
  login: (data: TenantLoginRequest) =>
    apiPost<TenantLoginResponse>('/api/v1/tenants/login', data),

  register: (data: TenantRegisterRequest) =>
    apiPost<TenantRegisterResponse>('/api/v1/tenants/register', data),

  getApiKeys: (tenantId: string) =>
    apiGet<ApiKey[]>(`/api/v1/tenants/${tenantId}/api-keys`),

  createApiKey: (tenantId: string, data: { name: string; permissions: string[] }) =>
    apiPost<{ raw_key: string; key: ApiKey }>(`/api/v1/tenants/${tenantId}/api-keys`, data),
}

// Storage helpers
const TENANT_TOKEN_KEY = 'tenant_token'
const TENANT_DATA_KEY = 'tenant'

export const getTenantToken = (): string | null => {
  if (typeof window === 'undefined') return null
  return sessionStorage.getItem(TENANT_TOKEN_KEY) || localStorage.getItem(TENANT_TOKEN_KEY)
}

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

export const saveTenantToken = (token: string): void => {
  if (typeof window === 'undefined') return
  sessionStorage.setItem(TENANT_TOKEN_KEY, token)
}

export const saveTenantData = (tenant: Tenant): void => {
  if (typeof window === 'undefined') return
  sessionStorage.setItem(TENANT_DATA_KEY, JSON.stringify(tenant))
}

export const clearTenantData = (): void => {
  if (typeof window === 'undefined') return
  sessionStorage.removeItem(TENANT_TOKEN_KEY)
  sessionStorage.removeItem(TENANT_DATA_KEY)
  localStorage.removeItem(TENANT_TOKEN_KEY)
  localStorage.removeItem(TENANT_DATA_KEY)
}
