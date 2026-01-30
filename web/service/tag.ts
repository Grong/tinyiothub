import { apiGet, apiPost, apiPut, apiDelete } from '@/lib/api-client'
import type { 
  Tag, 
  TagBinding, 
  CreateTagRequest, 
  UpdateTagRequest, 
  CreateTagBindingRequest, 
  BatchTagBindingRequest,
  TagStats 
} from '@/types'

// Re-export types for convenience
export type { Tag, TagBinding, CreateTagRequest, UpdateTagRequest, CreateTagBindingRequest, BatchTagBindingRequest, TagStats }

// Fetch tags by type
export const fetchTagList = async (type?: string): Promise<Tag[]> => {
  const params = type ? { type } : {}
  const response = await apiGet<Tag[]>('/tags', params)
  return response.result || []
}

// Get all tags
export const getAllTags = async (): Promise<Tag[]> => {
  const response = await apiGet<Tag[]>('/tags')
  return response.result || []
}

// Get tag by ID
export const getTag = async (tagId: string): Promise<Tag> => {
  const response = await apiGet<Tag>(`/tags/${tagId}`)
  if (!response.result) {
    throw new Error('Tag not found')
  }
  return response.result
}

// Create new tag
export const createTag = async (data: CreateTagRequest): Promise<Tag> => {
  const response = await apiPost<Tag>('/tags', data)
  if (!response.result) {
    throw new Error('Failed to create tag')
  }
  return response.result
}

// Update tag
export const updateTag = async (tagId: string, name: string): Promise<Tag> => {
  const response = await apiPut<Tag>(`/tags/${tagId}`, { name })
  if (!response.result) {
    throw new Error('Failed to update tag')
  }
  return response.result
}

// Delete tag
export const deleteTag = async (tagId: string): Promise<void> => {
  await apiDelete<void>(`/tags/${tagId}`)
}

// Create tag binding
export const createTagBinding = async (data: CreateTagBindingRequest): Promise<TagBinding> => {
  const response = await apiPost<TagBinding>('/tags/bindings', {
    tagId: data.tagId,
    targetId: data.targetId,
  })
  if (!response.result) {
    throw new Error('Failed to create tag binding')
  }
  return response.result
}

// Delete tag binding
export const deleteTagBinding = async (tagId: string, targetId: string): Promise<void> => {
  const params = new URLSearchParams({
    tag_id: tagId,
    target_id: targetId,
  })
  await apiDelete<void>(`/tags/bindings?${params.toString()}`)
}

// Get tags for a target (resource)
export const getResourceTags = async (targetId: string): Promise<Tag[]> => {
  const response = await apiGet<Tag[]>(`/tags/bindings/target/${targetId}`)
  return response.result || []
}

// Get bindings for a tag
export const getTagBindings = async (tagId: string): Promise<TagBinding[]> => {
  const response = await apiGet<TagBinding[]>(`/tags/bindings/tag/${tagId}`)
  return response.result || []
}

// Batch create tag bindings
export const batchCreateTagBindings = async (data: BatchTagBindingRequest): Promise<TagBinding[]> => {
  const response = await apiPost<TagBinding[]>('/tags/bindings/batch', {
    tag_ids: data.tagIds,
    target_id: data.targetId,
  })
  return response.result || []
}

// Batch delete tag bindings
export const batchDeleteTagBindings = async (targetId: string): Promise<void> => {
  const params = new URLSearchParams({
    target_id: targetId,
  })
  await apiDelete<void>(`/tags/bindings/batch?${params.toString()}`)
}

// Search tags
export const searchTags = async (query: string, type?: string): Promise<Tag[]> => {
  const params: any = { name: query }
  if (type) params.type = type
  
  const response = await apiGet<Tag[]>('/tags/search', params)
  return response.result || []
}

// Get tag statistics
export const getTagStats = async (): Promise<TagStats> => {
  const response = await apiGet<TagStats>('/tags/stats')
  return response.result || { total: 0, byType: {} }
}

// Legacy compatibility functions (updated to use new API)
export const createTagRelation = createTagBinding
export const deleteTagRelation = deleteTagBinding
export const getTagRelations = getTagBindings
export const batchCreateTagRelations = batchCreateTagBindings
export const batchDeleteTagRelations = batchDeleteTagBindings

export const bindTag = async (tagId: string, resourceId: string, resourceType: string): Promise<TagBinding> => {
  return createTagBinding({ tagId, targetId: resourceId })
}

export const unBindTag = async (tagId: string, resourceId: string, resourceType: string): Promise<void> => {
  return deleteTagBinding(tagId, resourceId)
}

export const batchBindTags = async (tagIds: string[], resourceId: string, resourceType: string): Promise<TagBinding[]> => {
  return batchCreateTagBindings({ 
    tagIds, 
    targetId: resourceId
  })
}

export const batchUnbindTags = async (resourceId: string, resourceType: string): Promise<void> => {
  return batchDeleteTagBindings(resourceId)
}