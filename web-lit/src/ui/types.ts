// web-lit/src/ui/types.ts
// Consolidated domain types — single source of truth

// === Auth ===
export interface User {
  id: string
  name: string
  email?: string
  phone?: string
  avatar?: string
  dateLastLogon?: string
  isDisabled: boolean
  parentId?: string
}

export interface UserProfile extends User {
  role?: string
  permissions?: string[]
  createdAt?: string
  updatedAt?: string
}

export interface LoginRequest {
  username: string
  password: string
}

export interface LoginResponse {
  accessToken: string
  tokenType: string
  expiresIn: number
  userInfo: User
}

export interface ChangePasswordRequest {
  oldPassword: string
  newPassword: string
}

// === Workspace ===
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

// === Device ===
export interface Device {
  id: string
  name: string
  displayName?: string
  deviceType?: string
  address?: string
  description?: string
  position?: string
  driverName?: string
  deviceModel?: string
  protocolType?: string
  protocol?: string
  factoryName?: string
  linkedData?: string
  driverOptions?: string
  state?: number
  parentId?: string
  productId?: string
  organizationId?: string
  createdAt?: string
  updatedAt?: string
  status?: 'online' | 'offline' | 'warning' | 'error' | 'maintenance'
  tags?: Tag[]
  properties?: DeviceProperty[]
  productName?: string
}

export interface DeviceProperty {
  id: string
  deviceId: string
  name: string
  displayName?: string
  value: any
  currentValue?: any
  dataType: string
  unit?: string
  description?: string
  updatedAt: string
  lastUpdateTime?: string
  alarmStatus?: number
  isReadOnly?: boolean
  readonly?: boolean
  minValue?: number
  maxValue?: number
}

export interface DeviceCommand {
  id: string
  deviceId: string
  name: string
  description?: string
  parameters: Record<string, unknown>
  createdAt: string
}

export interface DeviceListParams {
  page?: number
  pageSize?: number
  search?: string
  protocol?: string
  status?: string
  state?: string
  deviceType?: string
  driverName?: string
  isCreatedByMe?: boolean
  tagIds?: string[]
  name?: string
  productId?: string
  enabled?: boolean
}

export interface CreateDeviceRequest {
  name: string
  displayName?: string
  description?: string
  protocol?: string
  address?: string
  position?: string
  driverName?: string
  driverOptions?: Record<string, unknown>
  tags?: string[]
  type?: string
  propertyValues?: Record<string, unknown>
  enabledCommands?: string[]
}

export interface DeviceAlarm {
  id: string
  deviceId: string
  deviceName: string
  level: 'info' | 'warning' | 'error' | 'critical'
  message: string
  status: 'active' | 'acknowledged' | 'resolved'
  createdAt: string
  acknowledgedAt?: string
  resolvedAt?: string
}

// === Alarm ===
export type AlarmLevel = 'Info' | 'Warning' | 'Error' | 'Critical'
export type AlarmStatus = 'Active' | 'Acknowledged' | 'Resolved' | 'Suppressed'

export interface Alarm {
  id: string
  deviceId: string
  deviceName?: string
  propertyId?: string
  propertyName?: string
  ruleId?: string
  ruleName?: string
  alarmType: string
  alarmLevel: AlarmLevel
  message: string
  alarmValue?: string
  thresholdValue?: string
  alarmTime: string
  status: AlarmStatus
  isAcknowledged: boolean
  acknowledgedBy?: string
  acknowledgedAt?: string
  acknowledgedNote?: string
  isResolved: boolean
  resolvedBy?: string
  resolvedAt?: string
  resolvedNote?: string
  createdAt: string
}

export interface AlarmStatistics {
  totalCount: number
  activeCount: number
  acknowledgedCount: number
  resolvedCount: number
}

export interface AlarmQueryParams {
  deviceIds?: string[]
  levels?: AlarmLevel[]
  statuses?: AlarmStatus[]
  startTime?: string
  endTime?: string
  page?: number
  pageSize?: number
}

// === Dashboard ===
export interface DashboardStats {
  totalDevices: number
  onlineDevices: number
  offlineDevices: number
  activeAlarms: number
  criticalAlarms: number
  systemStatus: 'healthy' | 'warning' | 'error'
  systemUptime: number
  todayMessages: number
  monthlyGrowth: { devices: number; messages: number }
}

export interface DeviceStatusDistribution {
  online: number
  offline: number
  warning: number
  error: number
  maintenance: number
}

export interface RecentAlarm {
  id: string
  deviceId: string
  deviceName: string
  level: 'info' | 'warning' | 'error' | 'critical'
  message: string
  createdAt: string
  status: 'active' | 'acknowledged' | 'resolved'
}

export interface DashboardMetrics {
  cpu: number
  memory: number
  disk: number
  network: { inbound: number; outbound: number }
}

export interface QuickDevice {
  id: string
  name: string
  status: 'online' | 'offline' | 'error' | 'maintenance'
  lastSeen: string
  type: string
}

export interface DashboardData {
  stats: DashboardStats
  deviceDistribution: DeviceStatusDistribution
  dataTrends: DataTrend[]
  protocolUsage: ProtocolUsage[]
  recentAlarms: RecentAlarm[]
  systemMetrics: DashboardMetrics
  quickDevices: QuickDevice[]
}

export interface DataTrend {
  timestamp: string
  value: number
  label?: string
}

export interface ProtocolUsage {
  protocol: string
  count: number
  percentage: number
}

// === Tag ===
export interface Tag {
  id: string
  name: string
  type: string
  description?: string
  color?: string
  bindingCount?: number
  createdBy?: string
  createdAt: string
  updatedAt?: string
}

// === Template ===
export interface Template {
  id: string
  name: string
  displayName?: Record<string, string> | string
  description?: Record<string, string> | string
  category: string
  version: string
  author?: string
  manufacturer?: string
  deviceType?: string
  protocolType?: string
  driverName?: string
  isBuiltin: boolean
  tags: string[]
  configuration: Record<string, unknown>
  properties: TemplateProperty[]
  commands: TemplateCommand[]
  createdAt: string
  updatedAt: string
}

export interface TemplateProperty {
  id: string
  name: string
  displayName?: string
  description?: string
  dataType: string
  unit?: string
  defaultValue?: string
  minValue?: string
  maxValue?: string
  isReadOnly: boolean
  isRequired: boolean
}

export interface TemplateCommand {
  id: string
  name: string
  displayName?: string
  description?: string
  parameters: TemplateCommandParameter[]
  isRequired?: boolean
}

export interface TemplateCommandParameter {
  name: string
  displayName?: string
  description?: string
  dataType: string
  defaultValue?: string
  isRequired: boolean
}

export interface TemplateListParams {
  page?: number
  pageSize?: number
  keyword?: string
  category?: string
  manufacturer?: string
  protocolType?: string
  deviceType?: string
}

// === Agent / A2UI ===
export interface ChatMessage {
  id: string
  role: 'user' | 'assistant' | 'system'
  content: string
  timestamp: string
  surfaces?: Map<string, A2uiSurfaceState>
  isStreaming?: boolean
}

export interface A2uiSurfaceState {
  surfaceId: string
  title?: string
  components: A2uiComponentDescriptor[]
  dataModel: Record<string, unknown>
}

export interface A2uiComponentDescriptor {
  id: string
  type: string
  props?: Record<string, unknown>
  children?: A2uiComponentDescriptor[]
}

export type A2uiMessage =
  | { type: 'createSurface'; surface: A2uiSurfaceState }
  | { type: 'updateComponents'; components: A2uiComponentDescriptor[] }
  | { type: 'updateDataModel'; dataModel: Record<string, unknown> }
  | { type: 'deleteSurface'; surfaceId: string }
  | { type: 'callFunction'; functionCall: { name: string; args: Record<string, unknown> } }
  | { type: 'actionResponse'; response: { success: boolean; data?: unknown } }

export type SseEvent =
  | { type: 'delta'; content: string }
  | { type: 'a2ui'; message: A2uiMessage }
  | { type: 'final'; content: string }

// === Events ===
export const EventLevel = {
  Debug: 1,
  Info: 2,
  Warning: 3,
  Error: 4,
  Critical: 5
} as const

export type EventLevelValue = typeof EventLevel[keyof typeof EventLevel]

// === Pagination ===
export interface ApiResponse<T> {
  code: number
  msg: string
  result: T | null
}

export interface PaginatedResponse<T> {
  data: T[]
  pagination: {
    page: number
    pageSize: number
    totalPages: number
    totalCount: number
  }
}

// === Route ===
export type Route =
  | 'home'
  | 'signin'
  | 'register'
  | 'dashboard'
  | 'devices'
  | 'device-detail'
  | 'alarms'
  | 'monitoring'
  | 'agent'
  | 'settings'
  | 'tags'
  | 'templates'
  | 'marketplace'
  | 'marketplace-installed'

// === Notification ===
export interface Notification {
  id: string
  type: 'info' | 'warning' | 'error' | 'success'
  title: string
  message: string
  timestamp: number
  read: boolean
}

// === Driver ===
export interface Driver {
  name: string
  version?: string
  description?: string
  isLoaded: boolean
  category?: string
}

export interface DriverConfigOption {
  name: string
  label: string
  type: 'string' | 'number' | 'boolean' | 'select'
  defaultValue?: string
  required: boolean
  description?: string
  options?: string[]
}

// === SystemFeatures ===
export interface SystemFeatures {
  version?: string
  edition?: string
  buildTime?: string
  enableDeviceManagement?: boolean
  enableAlarmSystem?: boolean
  enableMonitoring?: boolean
  enableUserManagement?: boolean
  enableSystemSettings?: boolean
  apiPrefix?: string
  publicApiPrefix?: string
  maxDevices?: number
  maxUsers?: number
  maxAlarmRules?: number
  theme?: 'light' | 'dark' | 'system'
  language?: string
  timezone?: string
  enableAdvancedAnalytics?: boolean
  enableCustomDashboard?: boolean
  enableDataExport?: boolean
  enableApiAccess?: boolean
  enableTwoFactorAuth?: boolean
  sessionTimeout?: number
  passwordPolicy?: {
    minLength?: number
    requireUppercase?: boolean
    requireLowercase?: boolean
    requireNumbers?: boolean
    requireSpecialChars?: boolean
  }
  enableEmailNotifications?: boolean
  enableSmsNotifications?: boolean
  enableWebhookNotifications?: boolean
  systemStatus?: 'healthy' | 'degraded' | 'unhealthy'
  lastHealthCheck?: string
  licenseType?: 'community' | 'professional' | 'enterprise'
  licenseExpiry?: string
  licensedFeatures?: string[]
}

// === Tenant ===
export interface Tenant {
  id: string
  name: string
  slug: string
  status: string
  plan_id: string
  subscription_status: string
}
