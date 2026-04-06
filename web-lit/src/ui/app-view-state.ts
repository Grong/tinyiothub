// web-lit/src/ui/app-view-state.ts
// Single interface mapping every @state() property the root component will hold.

import type {
  Route, User, Workspace, Device, Alarm, DashboardData,
  ChatMessage, Tag, Template, Notification, Driver,
  AlarmQueryParams, DeviceListParams, TemplateListParams,
} from './types'

export interface AppViewState {
  // Global
  connected: boolean
  currentRoute: Route
  routeParams: Record<string, string>
  token: string | null
  user: User | null
  themeMode: 'dark' | 'light'
  navCollapsed: boolean
  searchQuery: string

  // Workspace
  workspaces: Workspace[]
  currentWorkspaceId: string | null

  // Auth
  authLoading: boolean

  // Devices
  devices: Device[]
  devicesLoading: boolean
  devicesPage: number
  devicesTotalPages: number
  devicesParams: DeviceListParams
  currentDevice: Device | null
  deviceDetailLoading: boolean

  // Alarms
  alarms: Alarm[]
  alarmsLoading: boolean
  alarmsPage: number
  alarmsTotalPages: number
  alarmQueryParams: AlarmQueryParams

  // Dashboard
  dashboardData: DashboardData | null
  dashboardLoading: boolean

  // Monitoring
  monitoringLoading: boolean

  // Agent
  chatMessages: ChatMessage[]
  streamingContent: string
  isStreaming: boolean
  sessionId: string | null

  // Tags
  tags: Tag[]
  tagsLoading: boolean

  // Templates
  templates: Template[]
  templatesLoading: boolean
  templatesPage: number
  templatesTotalPages: number
  templatesParams: TemplateListParams

  // Settings
  settingsLoading: boolean

  // Notifications
  notifications: Notification[]
  alarmCount: number

  // Drivers
  drivers: Driver[]

  // Marketplace (mock data for now)
  marketplaceLoading: boolean
}
