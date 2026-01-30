import { apiGet, apiPost } from '@/lib/api-client'
import { useQuery, useMutation, useQueryClient } from '@tanstack/react-query'
import { queryKeys } from '@/lib/query-keys'

// ==================== 类型定义 ====================

export interface AuthorInfo {
  name: string
  email: string
}

export interface TemplateMetadata {
  id: string
  name: string
  version: string
  category: string
  protocol: string
  manufacturer: string
  description: string
  tags: string[]
  author: AuthorInfo
  icon?: string
  downloads: number
  rating: number
  reviews: number
  license: string
  fileUrl: string
  checksum: string
  size: number
  createdAt: string
  updatedAt: string
}

export interface PlatformBinary {
  fileUrl: string
  checksum: string
  size: number
}

export interface DriverRequirements {
  minVersion: string
}

export interface DriverMetadata {
  id: string
  name: string
  version: string
  protocol: string
  description: string
  tags: string[]
  author: AuthorInfo
  icon?: string
  downloads: number
  rating: number
  reviews: number
  license: string
  homepage?: string
  documentation?: string
  platforms: Record<string, PlatformBinary>
  requirements: DriverRequirements
  createdAt: string
  updatedAt: string
}

export interface InstallRequest {
  version?: string
}

// ==================== API 调用函数 ====================

export const marketplaceApi = {
  // 模板市场
  getTemplates: () => apiGet<TemplateMetadata[]>('marketplace/templates'),
  
  getTemplate: (id: string) => apiGet<TemplateMetadata | null>(`marketplace/templates/${id}`),
  
  installTemplate: (id: string, data?: InstallRequest) => 
    apiPost<string>(`marketplace/templates/${id}/install`, data || {}),
  
  // 驱动市场
  getDrivers: () => apiGet<DriverMetadata[]>('marketplace/drivers'),
  
  getDriver: (id: string) => apiGet<DriverMetadata | null>(`marketplace/drivers/${id}`),
  
  installDriver: (id: string, data?: InstallRequest) => 
    apiPost<string>(`marketplace/drivers/${id}/install`, data || {}),
}

// ==================== React Query Hooks ====================

// 模板市场 Hooks
export const useMarketplaceTemplates = () => {
  return useQuery({
    queryKey: queryKeys.marketplace.templates,
    queryFn: async () => {
      const response = await marketplaceApi.getTemplates()
      return response.result || []
    },
  })
}

export const useMarketplaceTemplate = (id: string, enabled = true) => {
  return useQuery({
    queryKey: queryKeys.marketplace.template(id),
    queryFn: async () => {
      const response = await marketplaceApi.getTemplate(id)
      return response.result
    },
    enabled: enabled && !!id,
  })
}

export const useInstallTemplate = () => {
  const queryClient = useQueryClient()
  
  return useMutation({
    mutationFn: ({ id, data }: { id: string; data?: InstallRequest }) =>
      marketplaceApi.installTemplate(id, data),
    onSuccess: () => {
      // 刷新模板列表
      queryClient.invalidateQueries({ queryKey: queryKeys.templates.all })
      queryClient.invalidateQueries({ queryKey: queryKeys.marketplace.templates })
    },
  })
}

// 驱动市场 Hooks
export const useMarketplaceDrivers = () => {
  return useQuery({
    queryKey: queryKeys.marketplace.drivers,
    queryFn: async () => {
      const response = await marketplaceApi.getDrivers()
      return response.result || []
    },
  })
}

// 简化别名
export const useTemplates = useMarketplaceTemplates
export const useDrivers = useMarketplaceDrivers

export const useMarketplaceDriver = (id: string, enabled = true) => {
  return useQuery({
    queryKey: queryKeys.marketplace.driver(id),
    queryFn: async () => {
      const response = await marketplaceApi.getDriver(id)
      return response.result
    },
    enabled: enabled && !!id,
  })
}

export const useInstallDriver = () => {
  const queryClient = useQueryClient()
  
  return useMutation({
    mutationFn: ({ id, data }: { id: string; data?: InstallRequest }) =>
      marketplaceApi.installDriver(id, data),
    onSuccess: () => {
      // 刷新驱动列表
      queryClient.invalidateQueries({ queryKey: queryKeys.drivers.all })
      queryClient.invalidateQueries({ queryKey: queryKeys.marketplace.drivers })
    },
  })
}
