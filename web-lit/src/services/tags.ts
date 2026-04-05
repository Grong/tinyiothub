/**
 * 标签服务 - Tag management API
 */

import { apiGet } from '../lib/api-client'

export interface Tag {
  id: string
  name: string
  color: string
}

export const tagApi = {
  getTags: (type: 'device' | 'alarm' = 'device') =>
    apiGet<Tag[]>(`tags?type=${type}`),
}