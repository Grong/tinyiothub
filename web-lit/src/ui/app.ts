import { LitElement, html } from 'lit'
import { customElement, state } from 'lit/decorators.js'
import type { AppViewState } from './app-view-state'
import type { Route, User, Workspace, Device, Alarm, DashboardData, ChatMessage, Tag, Template, Notification, Driver, AlarmQueryParams, DeviceListParams, TemplateListParams } from './types'
import { handleConnected, handleDisconnected } from './app-lifecycle'
import { renderApp } from './app-render'
import { setupRouter } from './app-router'
import { initTheme } from './theme'

@customElement('tinyiothub-app')
export class App extends LitElement {
  // Global
  @state() connected = false
  @state() currentRoute: Route = 'home'
  @state() routeParams: Record<string, string> = {}
  @state() token: string | null = sessionStorage.getItem('auth-token')
  @state() user: User | null = null
  @state() themeMode: 'dark' | 'light' = 'dark'
  @state() navCollapsed = false
  @state() searchQuery = ''

  // Workspace
  @state() workspaces: Workspace[] = []
  @state() currentWorkspaceId: string | null = sessionStorage.getItem('current-workspace-id')

  // Auth
  @state() authLoading = false

  // Devices
  @state() devices: Device[] = []
  @state() devicesLoading = false
  @state() devicesPage = 1
  @state() devicesTotalPages = 1
  @state() devicesParams: DeviceListParams = {}
  @state() currentDevice: Device | null = null
  @state() deviceDetailLoading = false

  // Alarms
  @state() alarms: Alarm[] = []
  @state() alarmsLoading = false
  @state() alarmsPage = 1
  @state() alarmsTotalPages = 1
  @state() alarmQueryParams: AlarmQueryParams = {}

  // Dashboard
  @state() dashboardData: DashboardData | null = null
  @state() dashboardLoading = false

  // Monitoring
  @state() monitoringLoading = false

  // Agent
  @state() chatMessages: ChatMessage[] = []
  @state() streamingContent = ''
  @state() isStreaming = false
  @state() sessionId: string | null = null

  // Tags
  @state() tags: Tag[] = []
  @state() tagsLoading = false

  // Templates
  @state() templates: Template[] = []
  @state() templatesLoading = false
  @state() templatesPage = 1
  @state() templatesTotalPages = 1
  @state() templatesParams: TemplateListParams = {}

  // Settings
  @state() settingsLoading = false

  // Notifications
  @state() notifications: Notification[] = []
  @state() alarmCount = 0

  // Drivers
  @state() drivers: Driver[] = []

  // Marketplace
  @state() marketplaceLoading = false

  // Non-reactive
  private _removeRouter: (() => void) | null = null

  createRenderRoot() {
    return this
  }

  connectedCallback() {
    super.connectedCallback()
    this.themeMode = initTheme()
    handleConnected(this as unknown as AppViewState)
    this._removeRouter = setupRouter(this as unknown as Parameters<typeof setupRouter>[0])
  }

  disconnectedCallback() {
    this._removeRouter?.()
    handleDisconnected(this as unknown as AppViewState)
    super.disconnectedCallback()
  }

  render() {
    return renderApp(this as unknown as AppViewState)
  }
}

declare global {
  interface HTMLElementTagNameMap {
    'tinyiothub-app': App
  }
}
