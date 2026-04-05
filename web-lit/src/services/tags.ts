/**
 * 标签服务 - Tag management API
 */

import { apiGet, apiPost, apiDelete } from '../lib/api-client'
import type { Tag, TagBinding } from '../types/tag'

export type { Tag }

export const tagApi = {
  getTags: (type: 'device' | 'alarm' = 'device') =>
    apiGet<Tag[]>(`tags?type=${type}`),

  getResourceTags: (targetId: string) =>
    apiGet<Tag[]>(`tags/bindings/target/${targetId}`),

  createTag: (name: string, type: string = 'device') =>
    apiPost<Tag>('tags', { name, type }),

  bindTag: (tagId: string, targetId: string, targetType: string = 'device') =>
    apiPost<TagBinding>('tags/bindings', { tagId, targetId, targetType }),

  unbindTag: (tagId: string, targetId: string) =>
    apiDelete(`tags/bindings?tag_id=${tagId}&target_id=${targetId}`),
}
