import { apiGet, apiPost } from '@/lib/api-client'
import { useQuery, useMutation, useQueryClient } from '@tanstack/react-query'
import { queryKeys } from '@/lib/query-keys'
import { MARKETPLACE_API_PREFIX } from '@/config'

// 模板类型
export interface TemplateMetadata {
  id: string
  name: string
  version: string
  category: string
  protocol: string
  manufacturer: string
  description: string
  tags: string[]
  author: { name: string; email: string }
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

// 驱动类型
export interface DriverMetadata {
  id: string
  name: string
  version: string
  protocol: string
  description: string
  tags: string[]
  author: { name: string; email: string }
  icon?: string
  downloads: number
  rating: number
  reviews: number
  license: string
  homepage?: string
  documentation?: string
  platforms: Record<string, { fileUrl: string; checksum: string; size: number }>
  requirements: { minVersion: string }
  createdAt: string
  updatedAt: string
}

// API 函数
const marketplaceApi = {
  getTemplates: () => apiGet<TemplateMetadata[]>(`${MARKETPLACE_API_PREFIX}/v1/templates`),
  getTemplate: (id: string) => apiGet<TemplateMetadata | null>(`${MARKETPLACE_API_PREFIX}/v1/templates/${id}`),
  installTemplate: (id: string) => apiPost<string>(`${MARKETPLACE_API_PREFIX}/v1/templates/${id}/install`, {}),
  getDrivers: () => apiGet<DriverMetadata[]>(`${MARKETPLACE_API_PREFIX}/v1/drivers`),
  getDriver: (id: string) => apiGet<DriverMetadata | null>(`${MARKETPLACE_API_PREFIX}/v1/drivers/${id}`),
  installDriver: (id: string) => apiPost<string>(`${MARKETPLACE_API_PREFIX}/v1/drivers/${id}/install`, {}),
}

// React Query Hooks
export const useMarketplaceTemplates = () =>
  useQuery({
    queryKey: queryKeys.marketplace.templates,
    queryFn: async () => {
      const res = await marketplaceApi.getTemplates()
      return res.result || []
    },
  })

export const useMarketplaceTemplate = (id: string, enabled = true) =>
  useQuery({
    queryKey: queryKeys.marketplace.template(id),
    queryFn: async () => (await marketplaceApi.getTemplate(id)).result,
    enabled: enabled && !!id,
  })

export const useInstallTemplate = () => {
  const qc = useQueryClient()
  return useMutation({
    mutationFn: ({ id }: { id: string }) => marketplaceApi.installTemplate(id),
    onSuccess: () => {
      qc.invalidateQueries({ queryKey: queryKeys.templates.all })
      qc.invalidateQueries({ queryKey: queryKeys.marketplace.templates })
    },
  })
}

export const useMarketplaceDrivers = () =>
  useQuery({
    queryKey: queryKeys.marketplace.drivers,
    queryFn: async () => {
      const res = await marketplaceApi.getDrivers()
      return res.result || []
    },
  })

export const useMarketplaceDriver = (id: string, enabled = true) =>
  useQuery({
    queryKey: queryKeys.marketplace.driver(id),
    queryFn: async () => (await marketplaceApi.getDriver(id)).result,
    enabled: enabled && !!id,
  })

export const useInstallDriver = () => {
  const qc = useQueryClient()
  return useMutation({
    mutationFn: ({ id }: { id: string }) => marketplaceApi.installDriver(id),
    onSuccess: () => {
      qc.invalidateQueries({ queryKey: queryKeys.drivers.all })
      qc.invalidateQueries({ queryKey: queryKeys.marketplace.drivers })
    },
  })
}
