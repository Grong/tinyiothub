import { apiGet, apiPost, apiDelete } from '../lib/api-client'

export interface Workspace {
  id: string
  name: string
  description?: string
  tenantId: string
  agentId?: string
  deviceCount?: number
  createdAt: string
  updatedAt: string
}

export const workspaceApi = {
  list: () => apiGet<Workspace[]>('workspaces'),
  get: (id: string) => apiGet<Workspace>(`workspaces/${id}`),
  create: (data: { name: string; description?: string }) =>
    apiPost<Workspace>('workspaces', data),
  delete: (id: string) => apiDelete<void>(`workspaces/${id}`),
}
