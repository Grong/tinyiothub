// web-lit/src/ui/controllers/workspace.ts
import type { AppViewState } from '../app-view-state'
import type { Workspace } from '../types'
import { apiGet, apiPost, apiDelete, setWorkspaceId } from '../api-client'

export async function loadWorkspaces(host: AppViewState): Promise<void> {
  const res = await apiGet<Workspace[]>('workspaces')
  if (res.result) {
    host.workspaces = res.result
    if (!host.currentWorkspaceId && res.result.length > 0) {
      selectWorkspace(host, res.result[0].id)
    }
  }
}

export function selectWorkspace(host: AppViewState, id: string): void {
  host.currentWorkspaceId = id
  setWorkspaceId(id)
  sessionStorage.setItem('current-workspace-id', id)
}

export async function createWorkspace(host: AppViewState, name: string, description?: string): Promise<void> {
  const res = await apiPost<Workspace>('workspaces', { name, description })
  if (res.result) {
    host.workspaces = [...host.workspaces, res.result]
    selectWorkspace(host, res.result.id)
  }
}

export async function deleteWorkspace(host: AppViewState, id: string): Promise<void> {
  await apiDelete(`workspaces/${id}`)
  host.workspaces = host.workspaces.filter(w => w.id !== id)
  if (host.currentWorkspaceId === id && host.workspaces.length > 0) {
    selectWorkspace(host, host.workspaces[0].id)
  }
}
