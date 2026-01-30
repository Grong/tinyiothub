/**
 * 设备模板服务
 * 使用 TanStack Query 进行数据获取和状态管理
 */

import { useQuery, useMutation, useQueryClient } from '@tanstack/react-query'
import { apiGet, apiPost, type PaginatedResponse } from '@/lib/api-client'
import { queryKeys } from '@/lib/query-keys'

// 设备模板类型定义（API客户端已转换为camelCase）
export interface DeviceTemplate {
  id: string
  name: string
  displayName: string // 已转换为camelCase，但仍是JSON字符串
  description: string | null // 已转换为camelCase，但仍是JSON字符串
  version: string
  author?: string
  category: string
  manufacturer?: string
  deviceType: string // 已转换为camelCase
  protocolType?: string // 已转换为camelCase
  driverName?: string // 已转换为camelCase
  tags: string // JSON数组字符串
  deviceInfo: string // JSON对象字符串
  properties: string // JSON数组字符串
  commands: string // JSON数组字符串
  isBuiltin: number // 已转换为camelCase
  isActive: number // 已转换为camelCase
  createdAt: string // 已转换为camelCase
  updatedAt: string // 已转换为camelCase
}

// 处理后的设备模板类型（解析JSON字段后）
export interface ProcessedDeviceTemplate {
  id: string
  name: string
  displayName: Record<string, string> // 解析后的多语言对象
  description: Record<string, string> | null // 解析后的多语言对象
  version: string
  author?: string
  category: string
  manufacturer?: string
  deviceType: string
  protocolType?: string
  driverName?: string
  tags: string[]
  deviceInfo: DeviceInfo
  properties: PropertyTemplate[]
  commands: CommandTemplate[]
  isBuiltin: boolean
  isActive: boolean
  createdAt: string
  updatedAt: string
}

export interface DeviceInfo {
  defaultNamePattern: string
  defaultDisplayNamePattern?: string
  defaultDescription?: Record<string, string>
  defaultPosition?: string
  defaultDriverOptions?: string
  requiredFields: string[]
}

// 工具函数：检查字段是否必填
export const isFieldRequired = (deviceInfo: DeviceInfo | undefined, fieldName: string): boolean => {
  return deviceInfo?.requiredFields?.includes(fieldName) || false
}

export interface PropertyTemplate {
  name: string
  displayName: Record<string, string>
  description?: Record<string, string>
  dataType: string
  unit?: string
  minValue?: number
  maxValue?: number
  defaultValue?: string
  isReadOnly: boolean
  isRequired: boolean
  validationRules?: string
}

export interface CommandTemplate {
  name: string
  displayName: Record<string, string>
  description?: Record<string, string>
  parameters?: string
  parameterSchema?: string
  isRequired: boolean
}

export interface TemplateCategory {
  name: string
  displayName: Record<string, string> // 解析后的多语言对象
  description?: Record<string, string> // 解析后的多语言对象
  templateCount: number
}

export interface TemplateQueryParams {
  category?: string
  manufacturer?: string
  deviceType?: string
  protocolType?: string
  keyword?: string
  page?: number
  pageSize?: number
}

export interface DeviceCreationInput {
  name: string
  displayName?: string
  description?: string
  position?: string
  address?: string
  driverName?: string
  driverOptions?: string
  parentId?: string
  productId?: string
  organizationId?: string
  propertyValues: Record<string, string>
  enabledCommands: string[]
}

export interface DevicePreview {
  deviceInfo: any
  properties: any[]
  commands: any[]
  warnings: string[]
}

export interface ValidationResult {
  isValid: boolean
  errors: ValidationError[]
  warnings: ValidationWarning[]
}

export interface ValidationError {
  field: string
  message: string
  errorCode: string
}

export interface ValidationWarning {
  field: string
  message: string
  warningCode: string
}

export interface CreateDeviceFromTemplateRequest {
  templateId: string
  deviceInput: DeviceCreationInput
}

// API 调用函数
export const templateApi = {
  // 获取模板列表
  getTemplates: (params?: TemplateQueryParams) => 
    apiGet<DeviceTemplate[]>('device-templates', params),

  // 获取模板详情
  getTemplate: (id: string) => 
    apiGet<DeviceTemplate>(`device-templates/${id}`),

  // 获取模板分类
  getTemplateCategories: () => 
    apiGet<TemplateCategory[]>('device-templates/categories'),

  // 验证用户输入
  validateTemplate: (id: string, input: DeviceCreationInput) => 
    apiPost<ValidationResult>(`device-templates/${id}/validate`, input),

  // 预览设备创建
  previewDevice: (id: string, input: DeviceCreationInput) => 
    apiPost<DevicePreview>(`device-templates/${id}/preview`, input),

  // 基于模板创建设备
  createDeviceFromTemplate: (request: CreateDeviceFromTemplateRequest) => 
    apiPost<any>('devices/from-template', request),
}

// 数据转换工具函数
function parseJsonField<T>(jsonString: string | null, fallback: T): T {
  if (!jsonString) return fallback
  try {
    return JSON.parse(jsonString)
  } catch (error) {
    console.warn('Failed to parse JSON field:', jsonString, error)
    return fallback
  }
}

function transformDeviceTemplate(raw: DeviceTemplate): ProcessedDeviceTemplate {
  return {
    id: raw.id,
    name: raw.name,
    displayName: parseJsonField(raw.displayName, {}),
    description: parseJsonField(raw.description, null),
    version: raw.version,
    author: raw.author,
    category: raw.category,
    manufacturer: raw.manufacturer,
    deviceType: raw.deviceType,
    protocolType: raw.protocolType,
    driverName: raw.driverName,
    tags: parseJsonField(raw.tags, []),
    deviceInfo: parseJsonField(raw.deviceInfo, {} as DeviceInfo),
    properties: parseJsonField(raw.properties, []),
    commands: parseJsonField(raw.commands, []),
    isBuiltin: raw.isBuiltin === 1,
    isActive: raw.isActive === 1,
    createdAt: raw.createdAt,
    updatedAt: raw.updatedAt,
  }
}

// React Query Hooks

/**
 * 获取设备模板列表
 */
export const useTemplates = (params?: TemplateQueryParams) => {
  return useQuery({
    queryKey: queryKeys.templates?.list(params || {}) || ['device-templates', 'list', params],
    queryFn: async () => {
      // 提供默认分页参数
      const queryParams = {
        ...params,
        page: params?.page || 1,
        pageSize: params?.pageSize || 20,
      }
      const response = await templateApi.getTemplates(queryParams)
      
      // 转换原始数据为处理后的格式
      if (Array.isArray(response.result)) {
        return response.result.map(transformDeviceTemplate)
      }
      return []
    },
    staleTime: 1000 * 60 * 5, // 5分钟
  })
}

/**
 * 获取模板详情
 */
export const useTemplate = (id: string, enabled = true) => {
  return useQuery({
    queryKey: queryKeys.templates?.detail(id) || ['device-templates', 'detail', id],
    queryFn: async () => {
      const response = await templateApi.getTemplate(id)
      return response.result ? transformDeviceTemplate(response.result) : null
    },
    enabled: enabled && !!id,
    staleTime: 1000 * 60 * 5, // 5分钟
  })
}

/**
 * 获取模板分类
 */
export const useTemplateCategories = () => {
  return useQuery({
    queryKey: queryKeys.templates?.categories() || ['device-templates', 'categories'],
    queryFn: async () => {
      const response = await templateApi.getTemplateCategories()
      return response.result
    },
    staleTime: 1000 * 60 * 10, // 10分钟
  })
}

/**
 * 验证模板输入
 */
export const useValidateTemplate = () => {
  return useMutation({
    mutationFn: ({ id, input }: { id: string; input: DeviceCreationInput }) =>
      templateApi.validateTemplate(id, input),
  })
}

/**
 * 预览设备创建
 */
export const usePreviewDevice = () => {
  return useMutation({
    mutationFn: ({ id, input }: { id: string; input: DeviceCreationInput }) =>
      templateApi.previewDevice(id, input),
  })
}

/**
 * 基于模板创建设备
 */
export const useCreateDeviceFromTemplate = () => {
  const queryClient = useQueryClient()

  return useMutation({
    mutationFn: templateApi.createDeviceFromTemplate,
    onSuccess: () => {
      // 刷新设备列表
      queryClient.invalidateQueries({ queryKey: queryKeys.devices?.lists() || ['devices'] })
    },
  })
}

// 导出服务对象，供组件直接调用
export const templateService = {
  ...templateApi,
}