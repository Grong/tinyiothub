import { atom, computed } from 'nanostores'

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

export const $currentWorkspaceId = atom<string | null>(
  typeof window !== 'undefined' ? sessionStorage.getItem('workspace-id') : null
)
export const $workspaces = atom<Workspace[]>([])

export const $currentWorkspace = computed(
  [$currentWorkspaceId, $workspaces],
  (id, workspaces) => (id ? workspaces.find(w => w.id === id) ?? null : null)
)

// Module-level subscription persists workspace selection to sessionStorage.
// No cleanup needed — workspace-store module is never unloaded.
$currentWorkspaceId.subscribe(id => {
  if (typeof window !== 'undefined') {
    if (id) {
      sessionStorage.setItem('workspace-id', id)
    } else {
      sessionStorage.removeItem('workspace-id')
    }
  }
})

export function selectWorkspace(id: string | null) {
  $currentWorkspaceId.set(id)
}

export function setWorkspaces(workspaces: readonly Workspace[]) {
  $workspaces.set([...workspaces])
}
