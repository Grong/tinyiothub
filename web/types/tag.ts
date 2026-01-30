/**
 * 标签相关类型定义
 * 前端统一使用 camelCase 命名
 */

export interface Tag {
  id: string
  name: string
  type: string
  description?: string
  color?: string
  bindingCount?: number
  createdBy?: string
  createdAt: string
  updatedAt?: string
}

export interface TagBinding {
  id: string
  tagId: string
  targetId: string
  createdBy?: string
  createdAt: string
}

export interface CreateTagRequest {
  name: string
  type: string
  description?: string
  color?: string
}

export interface UpdateTagRequest {
  name?: string
}

export interface CreateTagBindingRequest {
  tagId: string
  targetId: string
}

export interface BatchTagBindingRequest {
  tagIds: string[]
  targetId: string
}

export interface TagStats {
  total: number
  byType: Record<string, number>
}