import { apiGet, apiPost } from '@/lib/api-client'
import { useQuery, useMutation, useQueryClient } from '@tanstack/react-query'
import { queryKeys } from '@/lib/query-keys'
import { API_PREFIX } from '@/config'

// 使用本地后端代理，避免跨域问题
const MARKETPLACE_API_PREFIX = `${API_PREFIX}/marketplace`

// API 响应分页结构
interface PaginatedResult<T> {
  items: T[]
  total: number
  page: number
  per_page: number
}

// API 返回的模板原始数据结构
interface ApiTemplate {
  name: string
  display_name: { zh: string; en: string }
  description: { zh: string; en: string }
  version: string
  author: string
  category: string
  manufacturer: string | null
  device_type: string
  protocol_type: string
  driver_name: string
  tags: string[]
  device_info?: {
    default_name_pattern: string
    default_display_name_pattern: { zh: string; en: string }
    default_description: string | null
    required_fields: string[]
  }
  properties?: any[]
  commands?: any[]
}

// API 返回的驱动原始数据结构
interface ApiDriver {
  id: string
  name: string
  version: string
  protocol: string
  description: string
  tags: string[]
  author_name: string
  author_email: string | null
  icon: string | null
  license: string
  homepage: string | null
  documentation: string | null
  platforms: Record<string, { fileUrl: string; checksum: string; size: number }> | null
  requirements: { minVersion: string } | null
  updated_at: string
}

// 前端使用的模板类型（适配后）
export interface TemplateMetadata {
  id: string
  name: string
  displayName: string
  description: string
  version: string
  category: string
  protocol: string
  manufacturer: string
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

// 前端使用的驱动类型
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

// 转换 API 模板数据为前端格式
function transformTemplate(apiTemplate: ApiTemplate): TemplateMetadata {
  return {
    id: apiTemplate.name,
    name: apiTemplate.display_name?.zh || apiTemplate.display_name?.en || apiTemplate.name,
    displayName: apiTemplate.display_name?.zh || apiTemplate.display_name?.en || apiTemplate.name,
    description: apiTemplate.description?.zh || apiTemplate.description?.en || '',
    version: apiTemplate.version,
    category: apiTemplate.category,
    protocol: apiTemplate.protocol_type,
    manufacturer: apiTemplate.manufacturer || '',
    tags: apiTemplate.tags,
    author: { name: apiTemplate.author, email: '' },
    downloads: 0,
    rating: 0,
    reviews: 0,
    license: 'MIT',
    fileUrl: '',
    checksum: '',
    size: 0,
    createdAt: '',
    updatedAt: '',
  }
}

// 转换 API 驱动数据为前端格式
function transformDriver(apiDriver: ApiDriver): DriverMetadata {
  return {
    id: apiDriver.id,
    name: apiDriver.name,
    version: apiDriver.version,
    protocol: apiDriver.protocol,
    description: apiDriver.description,
    tags: apiDriver.tags,
    author: { name: apiDriver.author_name, email: apiDriver.author_email || '' },
    icon: apiDriver.icon || undefined,
    downloads: 0,
    rating: 0,
    reviews: 0,
    license: apiDriver.license,
    homepage: apiDriver.homepage || undefined,
    documentation: apiDriver.documentation || undefined,
    platforms: apiDriver.platforms || {},
    requirements: apiDriver.requirements || { minVersion: '' },
    createdAt: apiDriver.updated_at,
    updatedAt: apiDriver.updated_at,
  }
}

// API 函数
const marketplaceApi = {
  getTemplates: () => apiGet<TemplateMetadata[]>(`${MARKETPLACE_API_PREFIX}/templates`),
  getTemplate: (id: string) => apiGet<TemplateMetadata | null>(`${MARKETPLACE_API_PREFIX}/templates/${id}`),
  installTemplate: (id: string) => apiPost<string>(`${MARKETPLACE_API_PREFIX}/templates/${id}/install`, {}),
  getDrivers: () => apiGet<DriverMetadata[]>(`${MARKETPLACE_API_PREFIX}/drivers`),
  getDriver: (id: string) => apiGet<DriverMetadata | null>(`${MARKETPLACE_API_PREFIX}/drivers/${id}`),
  installDriver: (id: string) => apiPost<string>(`${MARKETPLACE_API_PREFIX}/drivers/${id}/install`, {}),
}

// React Query Hooks
export const useMarketplaceTemplates = () =>
  useQuery({
    queryKey: queryKeys.marketplace.templates,
    queryFn: async () => {
      try {
        const res = await marketplaceApi.getTemplates()
        const result = res.result as PaginatedResult<ApiTemplate> | null
        return (result?.items || []).map(transformTemplate)
      } catch {
        return []
      }
    },
  })

export const useMarketplaceTemplate = (id: string, enabled = true) =>
  useQuery({
    queryKey: queryKeys.marketplace.template(id),
    queryFn: async () => {
      const res = await marketplaceApi.getTemplate(id)
      const result = res.result as ApiTemplate | null
      return result ? transformTemplate(result) : null
    },
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
      try {
        const res = await marketplaceApi.getDrivers()
        const result = res.result as PaginatedResult<ApiDriver> | null
        return (result?.items || []).map(transformDriver)
      } catch {
        return []
      }
    },
  })

export const useMarketplaceDriver = (id: string, enabled = true) =>
  useQuery({
    queryKey: queryKeys.marketplace.driver(id),
    queryFn: async () => {
      const res = await marketplaceApi.getDriver(id)
      const result = res.result as ApiDriver | null
      return result ? transformDriver(result) : null
    },
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
