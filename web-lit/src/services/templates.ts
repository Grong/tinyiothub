/**
 * 设备模板服务 - Pure async API functions
 */

import { apiGet, apiPost } from '../lib/api-client'

// Types
export interface DeviceTemplate {
  id: string
  name: string
  displayName: string
  description: string | null
  version: string
  author?: string
  category: string
  manufacturer?: string
  deviceType: string
  protocolType?: string
  driverName?: string
  tags: string
  deviceInfo: string
  properties: string
  commands: string
  isBuiltin: number
  isActive: number
  createdAt: string
  updatedAt: string
}

export interface ProcessedDeviceTemplate {
  id: string
  name: string
  displayName: Record<string, string>
  description: Record<string, string> | null
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
  displayName: Record<string, string>
  description?: Record<string, string>
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

// Pure async API functions
export const templateApi = {
  getTemplates: (params?: TemplateQueryParams) =>
    apiGet<DeviceTemplate[]>('device-templates', params),

  getTemplate: (id: string) =>
    apiGet<DeviceTemplate>(`device-templates/${id}`),

  getTemplateCategories: () =>
    apiGet<TemplateCategory[]>('device-templates/categories'),

  validateTemplate: (id: string, input: DeviceCreationInput) =>
    apiPost<ValidationResult>(`device-templates/${id}/validate`, input),

  previewDevice: (id: string, input: DeviceCreationInput) =>
    apiPost<DevicePreview>(`device-templates/${id}/preview`, input),

  createDeviceFromTemplate: (request: CreateDeviceFromTemplateRequest) =>
    apiPost<any>('devices/from-template', request),
}

// Utility functions
export const isFieldRequired = (deviceInfo: DeviceInfo | undefined, fieldName: string): boolean => {
  return deviceInfo?.requiredFields?.includes(fieldName) || false
}

function parseJsonField<T>(jsonString: string | null, fallback: T): T {
  if (!jsonString) return fallback
  try {
    return JSON.parse(jsonString)
  } catch (error) {
    console.warn('Failed to parse JSON field:', jsonString, error)
    return fallback
  }
}

export function transformDeviceTemplate(raw: DeviceTemplate): ProcessedDeviceTemplate {
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
