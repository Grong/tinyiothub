/**
 * 模板相关类型定义
 */

export interface Template {
  id: string
  name: string
  displayName?: Record<string, string> | string
  description?: Record<string, string> | string
  category: string
  version: string
  author?: string
  manufacturer?: string
  deviceType?: string
  protocolType?: string
  driverName?: string
  isBuiltin?: boolean
  tags?: string[]
  configuration?: Record<string, any>
  properties?: TemplateProperty[]
  commands?: TemplateCommand[]
  createdAt?: string
  updatedAt?: string
}

export interface TemplateProperty {
  id: string
  name: string
  displayName?: Record<string, string> | string
  description?: Record<string, string> | string
  dataType: string
  unit?: string
  defaultValue?: any
  minValue?: number
  maxValue?: number
  isReadOnly?: boolean
  isRequired?: boolean
}

export interface TemplateCommand {
  id: string
  name: string
  displayName?: Record<string, string> | string
  description?: Record<string, string> | string
  parameters?: TemplateCommandParameter[]
  isRequired?: boolean
}

export interface TemplateCommandParameter {
  name: string
  displayName?: Record<string, string> | string
  description?: Record<string, string> | string
  dataType: string
  defaultValue?: any
  isRequired?: boolean
}

export interface TemplateListParams {
  page?: number
  pageSize?: number
  keyword?: string
  category?: string
  manufacturer?: string
  protocolType?: string
  deviceType?: string
}

export interface CreateTemplateRequest {
  name: string
  displayName?: Record<string, string>
  description?: Record<string, string>
  category: string
  version: string
  author?: string
  manufacturer?: string
  deviceType?: string
  protocolType?: string
  driverName?: string
  configuration?: Record<string, any>
  properties?: TemplateProperty[]
  commands?: TemplateCommand[]
}

export interface UpdateTemplateRequest extends Partial<CreateTemplateRequest> {
  id: string
}