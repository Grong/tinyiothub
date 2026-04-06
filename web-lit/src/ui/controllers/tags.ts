// web-lit/src/ui/controllers/tags.ts
import type { AppViewState } from '../app-view-state'
import type { Tag } from '../types'
import { apiGet, apiPost, apiDelete } from '../api-client'

export async function loadTags(host: AppViewState, type: 'device' | 'alarm' = 'device'): Promise<void> {
  host.tagsLoading = true
  try {
    const res = await apiGet<Tag[]>('tags', { type })
    if (res.result) {
      host.tags = res.result
    }
  } finally {
    host.tagsLoading = false
  }
}

export async function createTag(host: AppViewState, name: string, type: 'device' | 'alarm' = 'device'): Promise<void> {
  const res = await apiPost<Tag>('tags', { name, type })
  if (res.result) {
    host.tags = [...host.tags, res.result]
  }
}

export async function deleteTag(host: AppViewState, tagId: string): Promise<void> {
  await apiDelete(`tags/${tagId}`)
  host.tags = host.tags.filter(t => t.id !== tagId)
}

export async function bindTag(host: AppViewState, tagId: string, targetId: string, targetType: string = 'device'): Promise<void> {
  await apiPost('tags/bindings', { tagId, targetId, targetType })
}

export async function unbindTag(host: AppViewState, tagId: string, targetId: string): Promise<void> {
  await apiDelete(`tags/bindings?tag_id=${tagId}&target_id=${targetId}`)
}
