// web-lit/src/ui/app-lifecycle.ts
import type { AppViewState } from './app-view-state'
import { setWorkspaceId } from './api-client'
import { applyTheme } from './theme'
import { loadWorkspaces } from './controllers/workspace'
import { loadDevices } from './controllers/devices'

type LifecycleHost = AppViewState

export function handleConnected(host: LifecycleHost): void {
  // Apply persisted theme
  applyTheme(host.themeMode)

  // Sync workspaceId to api-client
  if (host.currentWorkspaceId) {
    setWorkspaceId(host.currentWorkspaceId)
  }

  // Auth gate
  if (host.token) {
    host.connected = true
    // Fire-and-forget initial data loads
    loadWorkspaces(host).catch(console.error)
  } else {
    host.connected = false
  }

  // Listen for auth errors from api-client
  window.addEventListener('auth-error', () => {
    host.token = null
    host.user = null
    host.connected = false
    sessionStorage.removeItem('auth-token')
  })
}

export function handleDisconnected(host: LifecycleHost): void {
  // Cleanup if component is ever removed
}
