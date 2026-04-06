# web-lit Architecture Rewrite Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Rewrite web-lit frontend from nanostores + page-components to single-root @state + pure-function views, following openclaw/ui's proven pattern.

**Architecture:** One `<tinyiothub-app>` LitElement holds all `@state()` properties. Lifecycle logic extracted to `app-lifecycle.ts`, rendering to `renderApp(state)` in `app-render.ts`, routing via declarative `RouteConfig[]` in `app-router.ts`. Each page is a pure function `renderXxx(state: AppViewState)` returning Lit `html`. Controllers are stateless functions that call ApiClient and mutate state.

**Tech Stack:** Lit 3, TypeScript, Vite 8, no nanostores, no @lit-labs/router, no ky. Light DOM (`createRenderRoot() { return this }`).

---

## File Structure

```
web-lit/src/
├── main.ts                          # Entry: import styles + create App
├── styles.css                       # CSS entry: @import all CSS files
├── styles/
│   ├── base.css                     # PRESERVED — design tokens, themes (731 lines)
│   ├── layout.css                   # REFACTORED — shell layout only (~400 lines)
│   ├── layout.mobile.css            # NEW — mobile responsive (~200 lines)
│   ├── components.css               # REFACTORED — generic UI components (~800 lines)
│   └── iot.css                      # NEW — IoT-specific styles (~1500 lines)
├── ui/
│   ├── app.ts                       # Root component (~150 lines)
│   ├── app-view-state.ts            # AppViewState type (~120 lines)
│   ├── app-render.ts                # renderApp(state) (~200 lines)
│   ├── app-lifecycle.ts             # handleConnected/Disconnected (~80 lines)
│   ├── app-router.ts                # RouteConfig[] + matchRoute + setupRouter (~80 lines)
│   ├── app-defaults.ts              # Default constants (~30 lines)
│   ├── api-client.ts                # MOVED from lib/, decoupled from nanostores
│   ├── theme.ts                     # Theme management (~40 lines)
│   ├── icons.ts                     # SVG icon library (preserved from components/)
│   ├── types.ts                     # Consolidated domain types
│   ├── controllers/
│   │   ├── auth.ts                  # Login/logout/profile/token
│   │   ├── devices.ts               # Device CRUD + monitoring
│   │   ├── alarms.ts                # Alarm list/acknowledge/resolve
│   │   ├── dashboard.ts             # Dashboard data aggregation
│   │   ├── workspace.ts             # Workspace management
│   │   ├── agent.ts                 # Agent chat SSE + A2UI
│   │   └── monitoring.ts            # System monitoring
│   ├── views/
│   │   ├── home.ts                  # Static marketing page
│   │   ├── signin.ts                # Login form
│   │   ├── register.ts              # Registration form
│   │   ├── dashboard.ts             # Dashboard with stats/charts
│   │   ├── devices.ts               # Device list + filters
│   │   ├── device-detail.ts         # Device detail with tabs
│   │   ├── alarms.ts                # Alarm list
│   │   ├── monitoring.ts            # Monitoring overview
│   │   ├── agent.ts                 # Agent chat interface
│   │   ├── settings.ts              # User settings
│   │   ├── tags.ts                  # Tag management
│   │   ├── templates.ts             # Template management
│   │   └── marketplace.ts           # Marketplace + installed
│   └── components/                  # MOVED from src/components/
│       ├── sidebar.ts
│       ├── topbar.ts
│       ├── device-card.ts
│       ├── device-form.ts
│       ├── alarm-list.ts
│       ├── chat-input.ts
│       ├── chat-thread.ts
│       └── ... (existing components, updated to receive state via properties)
├── i18n/                            # PRESERVED
├── lib/
│   ├── navigate.ts                  # PRESERVED
│   ├── local-storage.ts             # PRESERVED
│   └── case-converter.ts            # PRESERVED
└── lib/config.ts                    # PRESERVED
```

## Task Dependencies

```
Task 1 (types) ──→ Task 2 (api-client) ──→ Task 3 (app-router) ──→ Task 4 (app-view-state)
                                                                          │
                   Task 5 (theme/icons)                                   │
                        │                                                 ▼
                        └──────────────────────→ Task 6 (app.ts) ──→ Task 7 (app-lifecycle)
                                                                          │
                   Task 8 (app-defaults)                                   │
                        │                                                 ▼
                        └──────────────────────→ Task 9 (app-render) ◄────┘
                                                                          │
                   Tasks 10-16 (controllers) ──→ Tasks 17-29 (views)      │
                                                                          │
                   Tasks 30-31 (CSS split)                                 │
                                                                          ▼
                   Task 32 (main.ts + styles.css) ──→ Task 33 (cleanup) ──→ Task 34 (verify build)
```

---

## Task 1: Consolidated Types — `ui/types.ts`

**Files:**
- Create: `web-lit/src/ui/types.ts`
- Reference: `src/types/device.ts`, `src/types/alarm.ts`, `src/types/user.ts`, `src/types/tag.ts`, `src/types/dashboard.ts`, `src/types/template.ts`, `src/types/system.ts`, `src/types/feature.ts`, `src/types/agent-types.ts`, `src/services/devices.ts`, `src/services/auth.ts`, `src/services/workspace.ts`, `src/services/dashboard.ts`

- [ ] **Step 1: Create `ui/types.ts` with consolidated domain types**

Read all existing type files. Merge duplicates. Keep the services/devices.ts version of `CreateDeviceRequest` (the one actually used by API calls). Rename the types/device.ts version to `DeviceListItem` if needed. Remove duplicate `ChangePasswordRequest`, `DeviceProperty` (agent-types version), duplicate `SystemFeatures`.

```ts
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
  linkedData?: Record<string, unknown>
  driverOptions?: Record<string, unknown>
  state?: string
  parentId?: string
  productId?: string
  organizationId?: string
  createdAt?: string
  updatedAt?: string
  status?: string
  tags?: Tag[]
  properties?: DeviceProperty[]
  productName?: string
}

export interface DeviceProperty {
  id: string
  deviceId: string
  name: string
  displayName?: string
  value: string
  currentValue?: string
  dataType: string
  unit?: string
  description?: string
  updatedAt?: string
  lastUpdateTime?: string
  alarmStatus?: string
  isReadOnly?: boolean
  readonly?: boolean
  minValue?: string
  maxValue?: string
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
  activeAlarms: number
  systemStatus: 'healthy' | 'warning' | 'error'
  systemUptime: string
  todayMessages: number
  monthlyGrowth: { devices: number; messages: number }
}

export interface DeviceStatusDistribution {
  online: number
  offline: number
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

// === Tenant ===
export interface Tenant {
  id: string
  name: string
  slug: string
  status: string
  plan_id: string
  subscription_status: string
}
```

- [ ] **Step 2: Verify TypeScript compiles**

Run: `cd web-lit && npx tsc --noEmit src/ui/types.ts`
Expected: No errors (some unused type warnings are OK).

- [ ] **Step 3: Commit**

```bash
git add web-lit/src/ui/types.ts
git commit -m "feat(rewrite): add consolidated types file ui/types.ts"
```

---

## Task 2: Decoupled API Client — `ui/api-client.ts`

**Files:**
- Create: `web-lit/src/ui/api-client.ts`
- Reference: `src/lib/api-client.ts` (337 lines)

- [ ] **Step 1: Create `ui/api-client.ts` decoupled from nanostores**

Copy the existing api-client, remove the nanostore import for `$currentWorkspaceId`. Instead, accept workspaceId via a module-level setter or pass it per-request. Keep the token refresh logic, snake_case conversion, and error handling.

```ts
// web-lit/src/ui/api-client.ts
import { keysToCamelCase, keysToSnakeCase, type KeysToCamelCase } from '../lib/case-converter'
import { API_PREFIX } from '../lib/config'

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

export class ApiError extends Error {
  constructor(
    message: string,
    public code: number,
    public data?: unknown,
    public status?: number
  ) {
    super(message)
    this.name = 'ApiError'
  }
}

// Module-level state (set by app-lifecycle on login/workspace change)
let _workspaceId: string | null = null
export function setWorkspaceId(id: string | null) {
  _workspaceId = id
}

function getAuthToken(): string | null {
  return sessionStorage.getItem('auth-token')
}

let refreshPromise: Promise<void> | null = null

async function refreshToken(): Promise<void> {
  if (!refreshPromise) {
    refreshPromise = (async () => {
      try {
        const token = getAuthToken()
        const res = await fetch(`${API_PREFIX}/auth/refresh`, {
          method: 'POST',
          headers: {
            'Content-Type': 'application/json',
            ...(token ? { Authorization: `Bearer ${token}` } : {}),
          },
        })
        if (!res.ok) throw new Error('Refresh failed')
        const data = await res.json()
        if (data.result?.accessToken) {
          sessionStorage.setItem('auth-token', data.result.accessToken)
        }
      } finally {
        refreshPromise = null
      }
    })()
  }
  return refreshPromise
}

async function request<T>(
  method: string,
  path: string,
  options: {
    body?: unknown
    params?: Record<string, unknown>
    headers?: Record<string, string>
    skipAuth?: boolean
  } = {}
): Promise<ApiResponse<T>> {
  const { body, params, headers = {}, skipAuth } = options

  let url = `${API_PREFIX}/${path.replace(/^\//, '')}`
  if (params) {
    const snakeParams = keysToSnakeCase(params)
    const searchParams = new URLSearchParams()
    for (const [key, value] of Object.entries(snakeParams)) {
      if (value != null) searchParams.append(key, String(value))
    }
    const qs = searchParams.toString()
    if (qs) url += `?${qs}`
  }

  const token = getAuthToken()
  const fetchHeaders: Record<string, string> = {
    'Content-Type': 'application/json',
    ...headers,
  }
  if (token && !skipAuth) {
    fetchHeaders['Authorization'] = `Bearer ${token}`
  }
  if (_workspaceId) {
    fetchHeaders['X-Workspace-Id'] = _workspaceId
  }

  const response = await fetch(url, {
    method,
    headers: fetchHeaders,
    body: body != null ? JSON.stringify(keysToSnakeCase(body)) : undefined,
  })

  if (response.status === 401 && !skipAuth) {
    try {
      await refreshToken()
      fetchHeaders['Authorization'] = `Bearer ${getAuthToken()}`
      const retryResponse = await fetch(url, {
        method,
        headers: fetchHeaders,
        body: body != null ? JSON.stringify(keysToSnakeCase(body)) : undefined,
      })
      if (!retryResponse.ok) {
        if (retryResponse.status === 401) {
          sessionStorage.removeItem('auth-token')
          window.dispatchEvent(new CustomEvent('auth-error'))
        }
        throw new ApiError('Unauthorized', -1, null, retryResponse.status)
      }
      const retryData = await retryResponse.json()
      return { ...retryData, result: keysToCamelCase(retryData.result) } as ApiResponse<T>
    } catch {
      sessionStorage.removeItem('auth-token')
      window.dispatchEvent(new CustomEvent('auth-error'))
      throw new ApiError('Session expired', -1, null, 401)
    }
  }

  if (!response.ok) {
    const errorData = await response.json().catch(() => ({}))
    throw new ApiError(
      errorData.msg || `HTTP ${response.status}`,
      errorData.code ?? -1,
      errorData.result,
      response.status
    )
  }

  const data = await response.json()
  if (data.code !== 0) {
    throw new ApiError(data.msg || 'API error', data.code, data.result)
  }

  return { ...data, result: keysToCamelCase(data.result) } as ApiResponse<T>
}

export const apiClient = {
  get: <T>(path: string, params?: Record<string, unknown>, headers?: Record<string, string>) =>
    request<T>('GET', path, { params, headers }),

  post: <T>(path: string, body?: unknown, headers?: Record<string, string>) =>
    request<T>('POST', path, { body, headers }),

  put: <T>(path: string, body?: unknown, headers?: Record<string, string>) =>
    request<T>('PUT', path, { body, headers }),

  delete: <T>(path: string, headers?: Record<string, string>) =>
    request<T>('DELETE', path, { headers }),

  patch: <T>(path: string, body?: unknown, headers?: Record<string, string>) =>
    request<T>('PATCH', path, { body, headers }),
}

export const apiGet = apiClient.get
export const apiPost = apiClient.post
export const apiPut = apiClient.put
export const apiDelete = apiClient.delete
export const apiPatch = apiClient.patch
```

- [ ] **Step 2: Verify TypeScript compiles**

Run: `cd web-lit && npx tsc --noEmit src/ui/api-client.ts`
Expected: No errors.

- [ ] **Step 3: Commit**

```bash
git add web-lit/src/ui/api-client.ts
git commit -m "feat(rewrite): add decoupled api-client in ui/api-client.ts"
```

---

## Task 3: Router — `ui/app-router.ts`

**Files:**
- Create: `web-lit/src/ui/app-router.ts`
- Reference: openclaw/ui `navigation.ts` pattern

- [ ] **Step 1: Create `ui/app-router.ts` with declarative route table**

```ts
// web-lit/src/ui/app-router.ts
import type { Route } from './types'
import { navigate } from '../lib/navigate'

export interface RouteConfig {
  path: string
  route: Route
  public?: boolean
  paramNames?: string[]
}

export const ROUTES: RouteConfig[] = [
  { path: '/', route: 'home', public: true },
  { path: '/signin', route: 'signin', public: true },
  { path: '/register', route: 'register', public: true },
  { path: '/dashboard', route: 'dashboard' },
  { path: '/devices', route: 'devices' },
  { path: '/devices/:id', route: 'device-detail', paramNames: ['id'] },
  { path: '/alarms', route: 'alarms' },
  { path: '/monitoring', route: 'monitoring' },
  { path: '/agent', route: 'agent' },
  { path: '/settings', route: 'settings' },
  { path: '/tags', route: 'tags' },
  { path: '/templates', route: 'templates' },
  { path: '/marketplace', route: 'marketplace', public: true },
  { path: '/marketplace/installed', route: 'marketplace-installed' },
]

export const PUBLIC_ROUTES: Route[] = ROUTES.filter(r => r.public).map(r => r.route)

export const DEFAULT_ROUTE: Route = 'home'

export interface MatchResult {
  route: Route
  params: Record<string, string>
}

export function matchRoute(pathname: string): MatchResult {
  const normalized = pathname.replace(/\/+$/, '') || '/'

  for (const config of ROUTES) {
    const configSegments = config.path.split('/').filter(Boolean)
    const urlSegments = normalized.split('/').filter(Boolean)

    if (configSegments.length !== urlSegments.length) continue

    const params: Record<string, string> = {}
    let matched = true

    for (let i = 0; i < configSegments.length; i++) {
      if (configSegments[i].startsWith(':')) {
        params[configSegments[i].slice(1)] = urlSegments[i]
      } else if (configSegments[i] !== urlSegments[i]) {
        matched = false
        break
      }
    }

    if (matched) {
      return { route: config.route, params }
    }
  }

  return { route: DEFAULT_ROUTE, params: {} }
}

export function pathForRoute(route: Route, params?: Record<string, string>): string {
  const config = ROUTES.find(r => r.route === route)
  if (!config) return '/'

  let path = config.path
  if (params) {
    for (const [key, value] of Object.entries(params)) {
      path = path.replace(`:${key}`, value)
    }
  }
  return path
}

export function isPublicRoute(route: Route): boolean {
  return PUBLIC_ROUTES.includes(route)
}

export type AppRouterHost = {
  currentRoute: Route
  routeParams: Record<string, string>
  token: string | null
  connected: boolean
}

export function setupRouter(host: AppRouterHost): () => void {
  function handlePopState() {
    const { route, params } = matchRoute(window.location.pathname)
    host.currentRoute = route
    host.routeParams = params
  }

  window.addEventListener('popstate', handlePopState)
  handlePopState()

  return () => {
    window.removeEventListener('popstate', handlePopState)
  }
}
```

- [ ] **Step 2: Verify TypeScript compiles**

Run: `cd web-lit && npx tsc --noEmit src/ui/app-router.ts`
Expected: No errors.

- [ ] **Step 3: Commit**

```bash
git add web-lit/src/ui/app-router.ts
git commit -m "feat(rewrite): add declarative router in ui/app-router.ts"
```

---

## Task 4: AppViewState — `ui/app-view-state.ts`

**Files:**
- Create: `web-lit/src/ui/app-view-state.ts`

- [ ] **Step 1: Create AppViewState type**

This type maps every `@state()` property the root component will hold. It mirrors what openclaw/ui does — a flat type listing all state fields, grouped by domain.

```ts
// web-lit/src/ui/app-view-state.ts
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
```

- [ ] **Step 2: Verify TypeScript compiles**

Run: `cd web-lit && npx tsc --noEmit src/ui/app-view-state.ts`
Expected: No errors.

- [ ] **Step 3: Commit**

```bash
git add web-lit/src/ui/app-view-state.ts
git commit -m "feat(rewrite): add AppViewState type definition"
```

---

## Task 5: Theme & Icons — `ui/theme.ts`, `ui/icons.ts`

**Files:**
- Create: `web-lit/src/ui/theme.ts`
- Create: `web-lit/src/ui/icons.ts`
- Reference: `src/stores/app-store.ts` (theme logic), `src/components/logo-icon.ts`

- [ ] **Step 1: Create `ui/theme.ts`**

```ts
// web-lit/src/ui/theme.ts
export type ThemeMode = 'dark' | 'light'

const STORAGE_KEY = 'theme-mode'

export function getStoredTheme(): ThemeMode {
  const stored = localStorage.getItem(STORAGE_KEY)
  if (stored === 'light' || stored === 'dark') return stored
  return 'dark'
}

export function applyTheme(mode: ThemeMode): void {
  document.documentElement.setAttribute('data-theme-mode', mode)
  localStorage.setItem(STORAGE_KEY, mode)
}

export function initTheme(): ThemeMode {
  const mode = getStoredTheme()
  applyTheme(mode)
  return mode
}

export function toggleTheme(current: ThemeMode): ThemeMode {
  const next = current === 'dark' ? 'light' : 'dark'
  applyTheme(next)
  return next
}
```

- [ ] **Step 2: Create `ui/icons.ts`**

Read `src/components/logo-icon.ts` and all inline SVGs used in app-sidebar.ts and app-header.ts. Create a single icons module with named SVG template exports.

```ts
// web-lit/src/ui/icons.ts
import { html } from 'lit'
import type { TemplateResult } from 'lit'

export type IconName =
  | 'home' | 'devices' | 'dashboard' | 'alarm' | 'monitoring'
  | 'agent' | 'settings' | 'tags' | 'templates' | 'marketplace'
  | 'menu' | 'close' | 'search' | 'user' | 'logout'
  | 'chevron-left' | 'chevron-right' | 'sun' | 'moon'

const icons: Record<IconName, TemplateResult> = {
  home: html`<svg xmlns="http://www.w3.org/2000/svg" width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="m3 9 9-7 9 7v11a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2z"/><polyline points="9 22 9 12 15 12 15 22"/></svg>`,
  devices: html`<svg xmlns="http://www.w3.org/2000/svg" width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><rect x="2" y="3" width="20" height="14" rx="2" ry="2"/><line x1="8" y1="21" x2="16" y2="21"/><line x1="12" y1="17" x2="12" y2="21"/></svg>`,
  dashboard: html`<svg xmlns="http://www.w3.org/2000/svg" width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><rect x="3" y="3" width="7" height="9"/><rect x="14" y="3" width="7" height="5"/><rect x="14" y="12" width="7" height="9"/><rect x="3" y="16" width="7" height="5"/></svg>`,
  alarm: html`<svg xmlns="http://www.w3.org/2000/svg" width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="m21.73 18-8-14a2 2 0 0 0-3.48 0l-8 14A2 2 0 0 0 4 21h16a2 2 0 0 0 1.73-3Z"/><line x1="12" y1="9" x2="12" y2="13"/><line x1="12" y1="17" x2="12.01" y2="17"/></svg>`,
  monitoring: html`<svg xmlns="http://www.w3.org/2000/svg" width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><polyline points="22 12 18 12 15 21 9 3 6 12 2 12"/></svg>`,
  agent: html`<svg xmlns="http://www.w3.org/2000/svg" width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M12 8V4H8"/><rect width="16" height="12" x="4" y="8" rx="2"/><path d="M2 14h2"/><path d="M20 14h2"/><path d="M15 13v2"/><path d="M9 13v2"/></svg>`,
  settings: html`<svg xmlns="http://www.w3.org/2000/svg" width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M12.22 2h-.44a2 2 0 0 0-2 2v.18a2 2 0 0 1-1 1.73l-.43.25a2 2 0 0 1-2 0l-.15-.08a2 2 0 0 0-2.73.73l-.22.38a2 2 0 0 0 .73 2.73l.15.1a2 2 0 0 1 1 1.72v.51a2 2 0 0 1-1 1.74l-.15.09a2 2 0 0 0-.73 2.73l.22.38a2 2 0 0 0 2.73.73l.15-.08a2 2 0 0 1 2 0l.43.25a2 2 0 0 1 1 1.73V20a2 2 0 0 0 2 2h.44a2 2 0 0 0 2-2v-.18a2 2 0 0 1 1-1.73l.43-.25a2 2 0 0 1 2 0l.15.08a2 2 0 0 0 2.73-.73l.22-.39a2 2 0 0 0-.73-2.73l-.15-.08a2 2 0 0 1-1-1.74v-.5a2 2 0 0 1 1-1.74l.15-.09a2 2 0 0 0 .73-2.73l-.22-.38a2 2 0 0 0-2.73-.73l-.15.08a2 2 0 0 1-2 0l-.43-.25a2 2 0 0 1-1-1.73V4a2 2 0 0 0-2-2z"/><circle cx="12" cy="12" r="3"/></svg>`,
  tags: html`<svg xmlns="http://www.w3.org/2000/svg" width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M12 2H2v10l9.29 9.29c.94.94 2.48.94 3.42 0l6.58-6.58c.94-.94.94-2.48 0-3.42L12 2Z"/><path d="M7 7h.01"/></svg>`,
  templates: html`<svg xmlns="http://www.w3.org/2000/svg" width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><rect x="3" y="3" width="18" height="18" rx="2" ry="2"/><line x1="3" y1="9" x2="21" y2="9"/><line x1="9" y1="21" x2="9" y2="9"/></svg>`,
  marketplace: html`<svg xmlns="http://www.w3.org/2000/svg" width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><circle cx="8" cy="21" r="1"/><circle cx="19" cy="21" r="1"/><path d="M2.05 2.05h2l2.66 12.42a2 2 0 0 0 2 1.58h9.78a2 2 0 0 0 1.95-1.57l1.65-7.43H5.12"/></svg>`,
  menu: html`<svg xmlns="http://www.w3.org/2000/svg" width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><line x1="4" y1="12" x2="20" y2="12"/><line x1="4" y1="6" x2="20" y2="6"/><line x1="4" y1="18" x2="20" y2="18"/></svg>`,
  close: html`<svg xmlns="http://www.w3.org/2000/svg" width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><line x1="18" y1="6" x2="6" y2="18"/><line x1="6" y1="6" x2="18" y2="18"/></svg>`,
  search: html`<svg xmlns="http://www.w3.org/2000/svg" width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><circle cx="11" cy="11" r="8"/><line x1="21" y1="21" x2="16.65" y2="16.65"/></svg>`,
  user: html`<svg xmlns="http://www.w3.org/2000/svg" width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M19 21v-2a4 4 0 0 0-4-4H9a4 4 0 0 0-4 4v2"/><circle cx="12" cy="7" r="4"/></svg>`,
  logout: html`<svg xmlns="http://www.w3.org/2000/svg" width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M9 21H5a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2h4"/><polyline points="16 17 21 12 16 7"/><line x1="21" y1="12" x2="9" y2="12"/></svg>`,
  'chevron-left': html`<svg xmlns="http://www.w3.org/2000/svg" width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><polyline points="15 18 9 12 15 6"/></svg>`,
  'chevron-right': html`<svg xmlns="http://www.w3.org/2000/svg" width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><polyline points="9 18 15 12 9 6"/></svg>`,
  sun: html`<svg xmlns="http://www.w3.org/2000/svg" width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><circle cx="12" cy="12" r="4"/><path d="M12 2v2"/><path d="M12 20v2"/><path d="m4.93 4.93 1.41 1.41"/><path d="m17.66 17.66 1.41 1.41"/><path d="M2 12h2"/><path d="M20 12h2"/><path d="m6.34 17.66-1.41 1.41"/><path d="m19.07 4.93-1.41 1.41"/></svg>`,
  moon: html`<svg xmlns="http://www.w3.org/2000/svg" width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M12 3a6 6 0 0 0 9 9 9 9 0 1 1-9-9Z"/></svg>`,
}

export function icon(name: IconName, size = 20) {
  return icons[name] ?? html`<span></span>`
}
```

- [ ] **Step 3: Verify TypeScript compiles**

Run: `cd web-lit && npx tsc --noEmit src/ui/theme.ts src/ui/icons.ts`
Expected: No errors.

- [ ] **Step 4: Commit**

```bash
git add web-lit/src/ui/theme.ts web-lit/src/ui/icons.ts
git commit -m "feat(rewrite): add theme manager and icon library"
```

---

## Task 6: Root Component — `ui/app.ts`

**Files:**
- Create: `web-lit/src/ui/app.ts`

- [ ] **Step 1: Create the root `<tinyiothub-app>` component**

```ts
// web-lit/src/ui/app.ts
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
```

- [ ] **Step 2: Verify TypeScript compiles**

Run: `cd web-lit && npx tsc --noEmit src/ui/app.ts`
Expected: May have errors from missing app-lifecycle/app-render — that's expected, those come next.

- [ ] **Step 3: Commit**

```bash
git add web-lit/src/ui/app.ts
git commit -m "feat(rewrite): add root component ui/app.ts with all @state properties"
```

---

## Task 7: App Lifecycle — `ui/app-lifecycle.ts`

**Files:**
- Create: `web-lit/src/ui/app-lifecycle.ts`

- [ ] **Step 1: Create lifecycle handler**

```ts
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
```

- [ ] **Step 2: Verify TypeScript compiles**

Run: `cd web-lit && npx tsc --noEmit src/ui/app-lifecycle.ts`
Expected: May error on missing controller imports — that's expected.

- [ ] **Step 3: Commit**

```bash
git add web-lit/src/ui/app-lifecycle.ts
git commit -m "feat(rewrite): add app-lifecycle.ts with handleConnected/disconnected"
```

---

## Task 8: App Defaults — `ui/app-defaults.ts`

**Files:**
- Create: `web-lit/src/ui/app-defaults.ts`

- [ ] **Step 1: Create defaults module**

```ts
// web-lit/src/ui/app-defaults.ts
import type { DeviceListParams, AlarmQueryParams, TemplateListParams } from './types'

export const DEFAULT_DEVICE_PARAMS: DeviceListParams = {
  page: 1,
  pageSize: 20,
}

export const DEFAULT_ALARM_PARAMS: AlarmQueryParams = {
  page: 1,
  pageSize: 20,
}

export const DEFAULT_TEMPLATE_PARAMS: TemplateListParams = {
  page: 1,
  pageSize: 20,
}

export const DEFAULT_DEVICES_PAGE_SIZE = 20
export const DEFAULT_ALARMS_PAGE_SIZE = 20
export const DEFAULT_TEMPLATES_PAGE_SIZE = 20
```

- [ ] **Step 2: Commit**

```bash
git add web-lit/src/ui/app-defaults.ts
git commit -m "feat(rewrite): add app-defaults.ts with default constants"
```

---

## Task 9: App Render — `ui/app-render.ts`

**Files:**
- Create: `web-lit/src/ui/app-render.ts`
- Reference: openclaw/ui `app-render.ts` pattern

- [ ] **Step 1: Create main render function**

```ts
// web-lit/src/ui/app-render.ts
import { html, nothing } from 'lit'
import type { AppViewState } from './app-view-state'
import { isPublicRoute } from './app-router'
import { renderSidebar } from './components/sidebar'
import { renderTopbar } from './components/topbar'

// Views
import { renderHome } from './views/home'
import { renderSignin } from './views/signin'
import { renderRegister } from './views/register'
import { renderDashboard } from './views/dashboard'
import { renderDevices } from './views/devices'
import { renderDeviceDetail } from './views/device-detail'
import { renderAlarms } from './views/alarms'
import { renderMonitoring } from './views/monitoring'
import { renderAgent } from './views/agent'
import { renderSettings } from './views/settings'
import { renderTags } from './views/tags'
import { renderTemplates } from './views/templates'
import { renderMarketplace } from './views/marketplace'

function renderRoute(state: AppViewState) {
  switch (state.currentRoute) {
    case 'home': return renderHome(state)
    case 'signin': return renderSignin(state)
    case 'register': return renderRegister(state)
    case 'dashboard': return renderDashboard(state)
    case 'devices': return renderDevices(state)
    case 'device-detail': return renderDeviceDetail(state)
    case 'alarms': return renderAlarms(state)
    case 'monitoring': return renderMonitoring(state)
    case 'agent': return renderAgent(state)
    case 'settings': return renderSettings(state)
    case 'tags': return renderTags(state)
    case 'templates': return renderTemplates(state)
    case 'marketplace':
    case 'marketplace-installed':
      return renderMarketplace(state)
    default:
      return renderHome(state)
  }
}

export function renderApp(state: AppViewState) {
  // Auth guard — not authenticated and not on a public route
  if (!state.token && !isPublicRoute(state.currentRoute)) {
    return html`<div class="app-shell">
      ${renderSignin(state)}
    </div>`
  }

  // Public routes render without sidebar/topbar chrome
  if (!state.token || isPublicRoute(state.currentRoute)) {
    return html`<div class="app-shell">
      ${renderRoute(state)}
    </div>`
  }

  // Full authenticated layout
  return html`<div class="app-shell">
    ${renderSidebar(state)}
    <div class="app-main">
      ${renderTopbar(state)}
      <div class="app-content">
        ${renderRoute(state)}
      </div>
    </div>
  </div>`
}
```

- [ ] **Step 2: Verify TypeScript compiles**

Run: `cd web-lit && npx tsc --noEmit src/ui/app-render.ts`
Expected: Errors from missing view/component imports — expected until those tasks complete.

- [ ] **Step 3: Commit**

```bash
git add web-lit/src/ui/app-render.ts
git commit -m "feat(rewrite): add app-render.ts with route dispatch and auth guard"
```

---

## Task 10: Auth Controller — `ui/controllers/auth.ts`

**Files:**
- Create: `web-lit/src/ui/controllers/auth.ts`
- Reference: `src/services/auth.ts`, `src/stores/auth-store.ts`

- [ ] **Step 1: Create auth controller**

```ts
// web-lit/src/ui/controllers/auth.ts
import type { AppViewState } from '../app-view-state'
import type { User, LoginResponse } from '../types'
import { apiPost } from '../api-client'
import { navigate } from '../../lib/navigate'

export async function login(host: AppViewState, username: string, password: string): Promise<void> {
  host.authLoading = true
  try {
    const res = await apiPost<LoginResponse>('auth/login', { username, password }, {}, true)
    if (res.result) {
      host.token = res.result.accessToken
      host.user = res.result.userInfo
      host.connected = true
      sessionStorage.setItem('auth-token', res.result.accessToken)
      navigate('/dashboard')
    }
  } finally {
    host.authLoading = false
  }
}

export async function logout(host: AppViewState): Promise<void> {
  try {
    await apiPost('auth/logout')
  } catch {
    // Logout best-effort
  }
  host.token = null
  host.user = null
  host.connected = false
  sessionStorage.removeItem('auth-token')
  navigate('/signin')
}

export async function loadProfile(host: AppViewState): Promise<void> {
  const res = await apiPost<User>('auth/session/profile')
  if (res.result) {
    host.user = res.result
  }
}

export async function updateProfile(host: AppViewState, data: Partial<User>): Promise<void> {
  const res = await apiPost<User>('auth/session/profile', data)
  if (res.result) {
    host.user = res.result
  }
}

export async function changePassword(host: AppViewState, oldPassword: string, newPassword: string): Promise<void> {
  await apiPost('auth/session/password', { oldPassword, newPassword })
}
```

Note: The `apiPost` signature in this controller shows `apiPost<T>(path, body, params?, skipAuth?)`. The actual api-client from Task 2 has `apiPost<T>(path, body?, headers?)`. For the login call (which needs skipAuth), we'll pass headers as `{}` and handle it. We need to add a `skipAuth` option to api-client — see Step 1b below.

- [ ] **Step 1b: Update api-client to support skipAuth**

```ts
// In web-lit/src/ui/api-client.ts, update the post method signature:
  post: <T>(path: string, body?: unknown, headers?: Record<string, string>, skipAuth?: boolean) =>
    request<T>('POST', path, { body, headers, skipAuth }),
```

And update the login call in auth controller to use `apiPost<LoginResponse>('auth/login', { username, password }, {}, true)`.

- [ ] **Step 2: Verify TypeScript compiles**

Run: `cd web-lit && npx tsc --noEmit`
Expected: May error on missing sibling controllers — expected.

- [ ] **Step 3: Commit**

```bash
git add web-lit/src/ui/controllers/auth.ts web-lit/src/ui/api-client.ts
git commit -m "feat(rewrite): add auth controller with login/logout/profile"
```

---

## Task 11: Workspace Controller — `ui/controllers/workspace.ts`

**Files:**
- Create: `web-lit/src/ui/controllers/workspace.ts`
- Reference: `src/services/workspace.ts`, `src/stores/workspace-store.ts`

- [ ] **Step 1: Create workspace controller**

```ts
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
```

- [ ] **Step 2: Commit**

```bash
git add web-lit/src/ui/controllers/workspace.ts
git commit -m "feat(rewrite): add workspace controller"
```

---

## Task 12: Devices Controller — `ui/controllers/devices.ts`

**Files:**
- Create: `web-lit/src/ui/controllers/devices.ts`
- Reference: `src/services/devices.ts`

- [ ] **Step 1: Create devices controller**

```ts
// web-lit/src/ui/controllers/devices.ts
import type { AppViewState } from '../app-view-state'
import type { Device, DeviceListParams, CreateDeviceRequest } from '../types'
import { apiGet, apiPost, apiPut, apiDelete } from '../api-client'

interface DeviceListResponse {
  items: Device[]
  total: number
  page: number
  pageSize: number
  totalPages: number
}

export async function loadDevices(host: AppViewState, params?: DeviceListParams): Promise<void> {
  host.devicesLoading = true
  try {
    const mergedParams = { ...host.devicesParams, ...params }
    host.devicesParams = mergedParams
    const res = await apiGet<DeviceListResponse>('devices', mergedParams as Record<string, unknown>)
    if (res.result) {
      host.devices = res.result.items
      host.devicesPage = res.result.page
      host.devicesTotalPages = res.result.totalPages
    }
  } finally {
    host.devicesLoading = false
  }
}

export async function loadDevice(host: AppViewState, id: string): Promise<void> {
  host.deviceDetailLoading = true
  try {
    const res = await apiGet<Device>(`devices/${id}`)
    if (res.result) {
      host.currentDevice = res.result
    }
  } finally {
    host.deviceDetailLoading = false
  }
}

export async function createDevice(host: AppViewState, data: CreateDeviceRequest): Promise<Device | null> {
  const res = await apiPost<Device>('devices', data)
  if (res.result) {
    host.devices = [res.result, ...host.devices]
    return res.result
  }
  return null
}

export async function updateDevice(host: AppViewState, id: string, data: Partial<CreateDeviceRequest>): Promise<void> {
  const res = await apiPut<Device>(`devices/${id}`, data)
  if (res.result) {
    host.devices = host.devices.map(d => d.id === id ? res.result! : d)
    if (host.currentDevice?.id === id) {
      host.currentDevice = res.result
    }
  }
}

export async function deleteDevice(host: AppViewState, id: string): Promise<void> {
  await apiDelete(`devices/${id}`)
  host.devices = host.devices.filter(d => d.id !== id)
  if (host.currentDevice?.id === id) {
    host.currentDevice = null
  }
}

export async function executeCommand(host: AppViewState, deviceId: string, commandId: string, parameters: Record<string, unknown>): Promise<void> {
  await apiPost(`devices/${deviceId}/commands/${commandId}/execute`, parameters)
}

export async function loadDrivers(host: AppViewState): Promise<void> {
  const res = await apiGet<Array<{ name: string; version?: string; description?: string; isLoaded: boolean; category?: string }>>('drivers/dynamic/list')
  if (res.result) {
    host.drivers = res.result
  }
}
```

- [ ] **Step 2: Commit**

```bash
git add web-lit/src/ui/controllers/devices.ts
git commit -m "feat(rewrite): add devices controller with CRUD + commands"
```

---

## Task 13: Alarms Controller — `ui/controllers/alarms.ts`

**Files:**
- Create: `web-lit/src/ui/controllers/alarms.ts`
- Reference: `src/services/devices.ts` (alarm methods), `src/types/alarm.ts`

- [ ] **Step 1: Create alarms controller**

```ts
// web-lit/src/ui/controllers/alarms.ts
import type { AppViewState } from '../app-view-state'
import type { Alarm, AlarmQueryParams } from '../types'
import { apiGet, apiPost } from '../api-client'

interface AlarmListResponse {
  items: Alarm[]
  total: number
  page: number
  pageSize: number
  totalPages: number
}

export async function loadAlarms(host: AppViewState, params?: AlarmQueryParams): Promise<void> {
  host.alarmsLoading = true
  try {
    const mergedParams = { ...host.alarmQueryParams, ...params }
    host.alarmQueryParams = mergedParams
    const res = await apiGet<AlarmListResponse>('alarms', mergedParams as Record<string, unknown>)
    if (res.result) {
      host.alarms = res.result.items
      host.alarmsPage = res.result.page
      host.alarmsTotalPages = res.result.totalPages
      host.alarmCount = res.result.items.filter(a => a.status === 'Active').length
    }
  } finally {
    host.alarmsLoading = false
  }
}

export async function acknowledgeAlarm(host: AppViewState, alarmId: string): Promise<void> {
  await apiPost(`alarms/${alarmId}/acknowledge`)
  host.alarms = host.alarms.map(a =>
    a.id === alarmId ? { ...a, status: 'Acknowledged' as const, isAcknowledged: true } : a
  )
}

export async function resolveAlarm(host: AppViewState, alarmId: string): Promise<void> {
  await apiPost(`alarms/${alarmId}/resolve`)
  host.alarms = host.alarms.map(a =>
    a.id === alarmId ? { ...a, status: 'Resolved' as const, isResolved: true } : a
  )
}
```

- [ ] **Step 2: Commit**

```bash
git add web-lit/src/ui/controllers/alarms.ts
git commit -m "feat(rewrite): add alarms controller"
```

---

## Task 14: Dashboard Controller — `ui/controllers/dashboard.ts`

**Files:**
- Create: `web-lit/src/ui/controllers/dashboard.ts`
- Reference: `src/services/dashboard.ts`

- [ ] **Step 1: Create dashboard controller**

```ts
// web-lit/src/ui/controllers/dashboard.ts
import type { AppViewState } from '../app-view-state'
import type { DashboardData } from '../types'
import { apiGet } from '../api-client'

export async function loadDashboard(host: AppViewState): Promise<void> {
  host.dashboardLoading = true
  try {
    const res = await apiGet<DashboardData>('dashboard')
    if (res.result) {
      host.dashboardData = res.result
    }
  } catch {
    // Fallback: compose from individual endpoints like the old service did
    try {
      const [statsRes, distRes, alarmsRes, quickRes] = await Promise.all([
        apiGet('monitoring/stats'),
        apiGet('devices/distribution'),
        apiGet('alarms/recent', { limit: 10 }),
        apiGet('devices/quick', { limit: 8 }),
      ])
      host.dashboardData = {
        stats: statsRes.result as DashboardData['stats'],
        deviceDistribution: distRes.result as DashboardData['deviceDistribution'],
        dataTrends: [],
        protocolUsage: [],
        recentAlarms: (alarmsRes.result as DashboardData['recentAlarms']) || [],
        systemMetrics: { cpu: 0, memory: 0, disk: 0, network: { inbound: 0, outbound: 0 } },
        quickDevices: (quickRes.result as DashboardData['quickDevices']) || [],
      }
    } catch {
      host.dashboardData = null
    }
  } finally {
    host.dashboardLoading = false
  }
}
```

- [ ] **Step 2: Commit**

```bash
git add web-lit/src/ui/controllers/dashboard.ts
git commit -m "feat(rewrite): add dashboard controller"
```

---

## Task 15: Agent Controller — `ui/controllers/agent.ts`

**Files:**
- Create: `web-lit/src/ui/controllers/agent.ts`
- Reference: `src/services/agent.ts`, `src/stores/agent-store.ts`

- [ ] **Step 1: Create agent controller with SSE streaming**

This is the critical one — the old code bypasses ApiClient with raw fetch. We integrate SSE properly.

```ts
// web-lit/src/ui/controllers/agent.ts
import type { AppViewState } from '../app-view-state'
import type { ChatMessage, A2uiMessage, SseEvent } from '../types'
import { apiPost } from '../api-client'
import { API_PREFIX } from '../../lib/config'

function getAuthToken(): string | null {
  return sessionStorage.getItem('auth-token')
}

function buildUrl(endpoint: string): string {
  return `${API_PREFIX}/${endpoint.replace(/^\//, '')}`
}

export async function sendAgentMessage(
  host: AppViewState,
  message: string
): Promise<void> {
  const userMsg: ChatMessage = {
    id: crypto.randomUUID(),
    role: 'user',
    content: message,
    timestamp: new Date().toISOString(),
  }
  host.chatMessages = [...host.chatMessages, userMsg]
  host.isStreaming = true
  host.streamingContent = ''

  const assistantMsg: ChatMessage = {
    id: crypto.randomUUID(),
    role: 'assistant',
    content: '',
    timestamp: new Date().toISOString(),
    isStreaming: true,
  }
  host.chatMessages = [...host.chatMessages, assistantMsg]

  try {
    const token = getAuthToken()
    const response = await fetch(buildUrl('agent/chat'), {
      method: 'POST',
      headers: {
        'Content-Type': 'application/json',
        ...(token ? { Authorization: `Bearer ${token}` } : {}),
      },
      body: JSON.stringify({
        message,
        session_id: host.sessionId,
      }),
    })

    if (!response.ok) {
      throw new Error(`Agent request failed: ${response.status}`)
    }

    const reader = response.body?.getReader()
    if (!reader) throw new Error('No response body')

    const decoder = new TextDecoder()
    let buffer = ''

    while (true) {
      const { done, value } = await reader.read()
      if (done) break

      buffer += decoder.decode(value, { stream: true })
      const lines = buffer.split('\n')
      buffer = lines.pop() || ''

      for (const line of lines) {
        if (!line.startsWith('data: ')) continue
        const data = line.slice(6)
        if (!data.trim()) continue

        try {
          const event = JSON.parse(data) as SseEvent
          switch (event.type) {
            case 'delta':
              host.streamingContent += event.content
              assistantMsg.content = host.streamingContent
              host.chatMessages = [...host.chatMessages]
              break
            case 'a2ui':
              handleA2uiMessage(host, assistantMsg, event.message)
              break
            case 'final':
              assistantMsg.content = event.content || host.streamingContent
              assistantMsg.isStreaming = false
              host.chatMessages = [...host.chatMessages]
              break
          }
        } catch {
          // Skip malformed JSON lines
        }
      }
    }
  } catch (error) {
    assistantMsg.content = `Error: ${error instanceof Error ? error.message : 'Unknown error'}`
    assistantMsg.isStreaming = false
    host.chatMessages = [...host.chatMessages]
  } finally {
    host.isStreaming = false
    host.streamingContent = ''
  }
}

function handleA2uiMessage(host: AppViewState, message: ChatMessage, a2uiMsg: A2uiMessage): void {
  if (!message.surfaces) {
    message.surfaces = new Map()
  }
  switch (a2uiMsg.type) {
    case 'createSurface':
      message.surfaces.set(a2uiMsg.surface.surfaceId, a2uiMsg.surface)
      break
    case 'updateComponents':
      // Update components in existing surface
      for (const surface of message.surfaces.values()) {
        for (const comp of a2uiMsg.components) {
          const idx = surface.components.findIndex(c => c.id === comp.id)
          if (idx >= 0) {
            surface.components[idx] = comp
          } else {
            surface.components.push(comp)
          }
        }
      }
      break
    case 'updateDataModel':
      for (const surface of message.surfaces.values()) {
        surface.dataModel = { ...surface.dataModel, ...a2uiMsg.dataModel }
      }
      break
    case 'deleteSurface':
      message.surfaces.delete(a2uiMsg.surfaceId)
      break
  }
  host.chatMessages = [...host.chatMessages]
}

export async function sendAgentAction(
  host: AppViewState,
  componentId: string,
  eventType: string,
  payload: Record<string, unknown>
): Promise<void> {
  await apiPost('agent/action', {
    session_id: host.sessionId,
    component_id: componentId,
    event_type: eventType,
    ...payload,
  })
}

export function clearChat(host: AppViewState): void {
  host.chatMessages = []
  host.sessionId = null
  host.streamingContent = ''
  host.isStreaming = false
}
```

- [ ] **Step 2: Commit**

```bash
git add web-lit/src/ui/controllers/agent.ts
git commit -m "feat(rewrite): add agent controller with SSE streaming"
```

---

## Task 16: Monitoring & Tags Controllers

**Files:**
- Create: `web-lit/src/ui/controllers/monitoring.ts`
- Create: `web-lit/src/ui/controllers/tags.ts`

- [ ] **Step 1: Create monitoring controller**

```ts
// web-lit/src/ui/controllers/monitoring.ts
import type { AppViewState } from '../app-view-state'
import { apiGet } from '../api-client'

export async function loadMonitoringData(host: AppViewState): Promise<void> {
  host.monitoringLoading = true
  try {
    const res = await apiGet('monitoring/metrics')
    // Store in dashboard for now — monitoring reuses dashboard metrics
    if (res.result && host.dashboardData) {
      host.dashboardData.systemMetrics = res.result as typeof host.dashboardData.systemMetrics
    }
  } finally {
    host.monitoringLoading = false
  }
}
```

- [ ] **Step 2: Create tags controller**

```ts
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
```

- [ ] **Step 3: Commit**

```bash
git add web-lit/src/ui/controllers/monitoring.ts web-lit/src/ui/controllers/tags.ts
git commit -m "feat(rewrite): add monitoring and tags controllers"
```

---

## Task 17: Shell Components — `ui/components/sidebar.ts` + `topbar.ts`

**Files:**
- Create: `web-lit/src/ui/components/sidebar.ts`
- Create: `web-lit/src/ui/components/topbar.ts`
- Reference: `src/components/app-sidebar.ts`, `src/components/app-header.ts`

- [ ] **Step 1: Create sidebar render function**

```ts
// web-lit/src/ui/components/sidebar.ts
import { html } from 'lit'
import type { AppViewState } from '../app-view-state'
import type { Route } from '../types'
import { icon } from '../icons'
import { navigate } from '../../lib/navigate'
import { logout } from '../controllers/auth'

interface NavItem {
  route: Route
  label: string
  icon: string
}

const NAV_ITEMS: NavItem[] = [
  { route: 'dashboard', label: 'Dashboard', icon: 'dashboard' },
  { route: 'devices', label: 'Devices', icon: 'devices' },
  { route: 'alarms', label: 'Alarms', icon: 'alarm' },
  { route: 'monitoring', label: 'Monitoring', icon: 'monitoring' },
  { route: 'agent', label: 'Agent', icon: 'agent' },
  { route: 'tags', label: 'Tags', icon: 'tags' },
  { route: 'templates', label: 'Templates', icon: 'templates' },
  { route: 'marketplace', label: 'Marketplace', icon: 'marketplace' },
  { route: 'settings', label: 'Settings', icon: 'settings' },
]

function handleNav(route: Route) {
  const paths: Record<Route, string> = {
    home: '/', signin: '/signin', register: '/register',
    dashboard: '/dashboard', devices: '/devices', 'device-detail': '/devices',
    alarms: '/alarms', monitoring: '/monitoring', agent: '/agent',
    settings: '/settings', tags: '/tags', templates: '/templates',
    marketplace: '/marketplace', 'marketplace-installed': '/marketplace/installed',
  }
  navigate(paths[route] || '/')
}

export function renderSidebar(state: AppViewState) {
  return html`
    <nav class="sidebar ${state.navCollapsed ? 'collapsed' : ''}">
      <div class="sidebar-brand">
        <logo-icon></logo-icon>
        ${state.navCollapsed ? nothing : html`<span class="sidebar-title">TinyIoTHub</span>`}
      </div>
      <div class="sidebar-nav">
        ${NAV_ITEMS.map(item => html`
          <button
            class="nav-item ${state.currentRoute === item.route ? 'active' : ''}"
            @click=${() => handleNav(item.route)}
            title=${item.label}
          >
            ${icon(item.icon as any)}
            ${state.navCollapsed ? nothing : html`<span class="nav-label">${item.label}</span>`}
          </button>
        `)}
      </div>
      <div class="sidebar-footer">
        <button class="nav-item" @click=${() => { state.navCollapsed = !state.navCollapsed }} title="Toggle sidebar">
          ${icon(state.navCollapsed ? 'chevron-right' : 'chevron-left')}
        </button>
      </div>
    </nav>
  `
}
```

Note: `nothing` is imported from `lit`. Let me fix the import.

```ts
import { html, nothing } from 'lit'
```

- [ ] **Step 2: Create topbar render function**

```ts
// web-lit/src/ui/components/topbar.ts
import { html, nothing } from 'lit'
import type { AppViewState } from '../app-view-state'
import { icon } from '../icons'
import { toggleTheme } from '../theme'
import { logout } from '../controllers/auth'
import { navigate } from '../../lib/navigate'

export function renderTopbar(state: AppViewState) {
  return html`
    <header class="topbar">
      <div class="topbar-left">
        <input
          type="text"
          class="topbar-search"
          placeholder="Search..."
          .value=${state.searchQuery}
          @input=${(e: Event) => { state.searchQuery = (e.target as HTMLInputElement).value }}
        />
      </div>
      <div class="topbar-right">
        <button class="topbar-btn" @click=${() => { state.themeMode = toggleTheme(state.themeMode) }} title="Toggle theme">
          ${icon(state.themeMode === 'dark' ? 'sun' : 'moon')}
        </button>
        ${state.user ? html`
          <div class="topbar-user">
            <span class="topbar-username">${state.user.name}</span>
            <button class="topbar-btn" @click=${() => logout(state)} title="Logout">
              ${icon('logout')}
            </button>
          </div>
        ` : nothing}
      </div>
    </header>
  `
}
```

- [ ] **Step 3: Commit**

```bash
git add web-lit/src/ui/components/sidebar.ts web-lit/src/ui/components/topbar.ts
git commit -m "feat(rewrite): add sidebar and topbar shell components"
```

---

## Task 18: Static Views — home, signin, register

**Files:**
- Create: `web-lit/src/ui/views/home.ts`
- Create: `web-lit/src/ui/views/signin.ts`
- Create: `web-lit/src/ui/views/register.ts`
- Reference: `src/pages/home-page.ts`, `src/pages/signin-page.ts`, `src/pages/register-page.ts`

- [ ] **Step 1: Create `views/home.ts`**

Read `src/pages/home-page.ts` (270 lines, static marketing page). Extract the template into a pure function.

```ts
// web-lit/src/ui/views/home.ts
import { html } from 'lit'
import type { AppViewState } from '../app-view-state'
import { navigate } from '../../lib/navigate'

export function renderHome(_state: AppViewState) {
  return html`
    <div class="home-page">
      <div class="home-hero">
        <h1>TinyIoTHub</h1>
        <p>Edge IoT Gateway Management Platform</p>
        <div class="home-actions">
          <button class="btn btn-primary" @click=${() => navigate('/signin')}>Sign In</button>
          <button class="btn btn-secondary" @click=${() => navigate('/register')}>Register</button>
        </div>
      </div>
      <div class="home-features">
        <div class="feature-card">
          <h3>Device Management</h3>
          <p>Manage IoT devices across multiple protocols</p>
        </div>
        <div class="feature-card">
          <h3>Real-time Monitoring</h3>
          <p>Monitor device status and performance in real-time</p>
        </div>
        <div class="feature-card">
          <h3>AI Agent</h3>
          <p>Intelligent agent for device management and automation</p>
        </div>
      </div>
    </div>
  `
}
```

- [ ] **Step 2: Create `views/signin.ts`**

Read `src/pages/signin-page.ts` (341 lines). Extract the form + login logic into a pure view that calls the auth controller.

```ts
// web-lit/src/ui/views/signin.ts
import { html } from 'lit'
import type { AppViewState } from '../app-view-state'
import { login } from '../controllers/auth'
import { navigate } from '../../lib/navigate'

export function renderSignin(state: AppViewState) {
  let username = ''
  let password = ''
  let error = ''

  function handleSubmit(e: Event) {
    e.preventDefault()
    login(state, username, password).catch(err => {
      error = err instanceof Error ? err.message : 'Login failed'
    })
  }

  return html`
    <div class="auth-page">
      <div class="auth-card">
        <h2>Sign In</h2>
        ${error ? html`<div class="callout callout-error">${error}</div>` : ''}
        <form @submit=${handleSubmit}>
          <div class="form-group">
            <label for="username">Username</label>
            <input id="username" type="text" class="form-field" required
              @input=${(e: Event) => { username = (e.target as HTMLInputElement).value }}
            />
          </div>
          <div class="form-group">
            <label for="password">Password</label>
            <input id="password" type="password" class="form-field" required
              @input=${(e: Event) => { password = (e.target as HTMLInputElement).value }}
            />
          </div>
          <button type="submit" class="btn btn-primary btn-full" ?disabled=${state.authLoading}>
            ${state.authLoading ? 'Signing in...' : 'Sign In'}
          </button>
        </form>
        <p class="auth-footer">
          Don't have an account? <a href="/register" @click=${(e: Event) => { e.preventDefault(); navigate('/register') }}>Register</a>
        </p>
      </div>
    </div>
  `
}
```

- [ ] **Step 3: Create `views/register.ts`**

```ts
// web-lit/src/ui/views/register.ts
import { html } from 'lit'
import type { AppViewState } from '../app-view-state'
import { apiPost } from '../api-client'
import { navigate } from '../../lib/navigate'

export function renderRegister(state: AppViewState) {
  let name = ''
  let username = ''
  let password = ''
  let email = ''
  let error = ''
  let loading = false

  async function handleSubmit(e: Event) {
    e.preventDefault()
    loading = true
    error = ''
    try {
      await apiPost('auth/register', { name, username, password, email })
      navigate('/signin')
    } catch (err) {
      error = err instanceof Error ? err.message : 'Registration failed'
    } finally {
      loading = false
    }
  }

  return html`
    <div class="auth-page">
      <div class="auth-card">
        <h2>Register</h2>
        ${error ? html`<div class="callout callout-error">${error}</div>` : ''}
        <form @submit=${handleSubmit}>
          <div class="form-group">
            <label for="name">Name</label>
            <input id="name" type="text" class="form-field" required
              @input=${(e: Event) => { name = (e.target as HTMLInputElement).value }}
            />
          </div>
          <div class="form-group">
            <label for="username">Username</label>
            <input id="username" type="text" class="form-field" required
              @input=${(e: Event) => { username = (e.target as HTMLInputElement).value }}
            />
          </div>
          <div class="form-group">
            <label for="email">Email</label>
            <input id="email" type="email" class="form-field"
              @input=${(e: Event) => { email = (e.target as HTMLInputElement).value }}
            />
          </div>
          <div class="form-group">
            <label for="password">Password</label>
            <input id="password" type="password" class="form-field" required
              @input=${(e: Event) => { password = (e.target as HTMLInputElement).value }}
            />
          </div>
          <button type="submit" class="btn btn-primary btn-full" ?disabled=${loading}>
            ${loading ? 'Registering...' : 'Register'}
          </button>
        </form>
        <p class="auth-footer">
          Already have an account? <a href="/signin" @click=${(e: Event) => { e.preventDefault(); navigate('/signin') }}>Sign In</a>
        </p>
      </div>
    </div>
  `
}
```

- [ ] **Step 4: Commit**

```bash
git add web-lit/src/ui/views/home.ts web-lit/src/ui/views/signin.ts web-lit/src/ui/views/register.ts
git commit -m "feat(rewrite): add home, signin, register views"
```

---

## Task 19: Dashboard View — `ui/views/dashboard.ts`

**Files:**
- Create: `web-lit/src/ui/views/dashboard.ts`
- Reference: `src/pages/dashboard-page.ts` (280 lines)

- [ ] **Step 1: Create dashboard view**

Read the existing dashboard-page.ts for its template structure. Convert to pure function.

```ts
// web-lit/src/ui/views/dashboard.ts
import { html, nothing } from 'lit'
import type { AppViewState } from '../app-view-state'
import { loadDashboard } from '../controllers/dashboard'

export function renderDashboard(state: AppViewState) {
  // Trigger load on first render if data is null
  if (!state.dashboardData && !state.dashboardLoading) {
    loadDashboard(state).catch(console.error)
  }

  if (state.dashboardLoading) {
    return html`<div class="page-loading"><skeleton></skeleton></div>`
  }

  const data = state.dashboardData
  if (!data) {
    return html`<div class="page-empty">No dashboard data available</div>`
  }

  return html`
    <div class="dashboard-page">
      <h2 class="page-title">Dashboard</h2>

      <div class="stats-grid">
        <div class="stat-card">
          <div class="stat-value">${data.stats.totalDevices}</div>
          <div class="stat-label">Total Devices</div>
        </div>
        <div class="stat-card">
          <div class="stat-value">${data.stats.onlineDevices}</div>
          <div class="stat-label">Online</div>
        </div>
        <div class="stat-card">
          <div class="stat-value">${data.stats.activeAlarms}</div>
          <div class="stat-label">Active Alarms</div>
        </div>
        <div class="stat-card">
          <div class="stat-value">${data.stats.todayMessages}</div>
          <div class="stat-label">Messages Today</div>
        </div>
      </div>

      <div class="dashboard-grid">
        <div class="card">
          <h3>Device Distribution</h3>
          <div class="distribution-chart">
            <span>Online: ${data.deviceDistribution.online}</span>
            <span>Offline: ${data.deviceDistribution.offline}</span>
            <span>Error: ${data.deviceDistribution.error}</span>
          </div>
        </div>
        <div class="card">
          <h3>Recent Alarms</h3>
          ${data.recentAlarms.length === 0
            ? html`<p class="muted">No recent alarms</p>`
            : html`<ul class="alarm-list">
                ${data.recentAlarms.map(a => html`
                  <li class="alarm-item alarm-${a.level}">
                    <span class="alarm-device">${a.deviceName}</span>
                    <span class="alarm-message">${a.message}</span>
                  </li>
                `)}
              </ul>`
          }
        </div>
        <div class="card">
          <h3>Quick Devices</h3>
          ${data.quickDevices.map(d => html`
            <div class="quick-device">
              <span class="status-dot status-${d.status}"></span>
              <span>${d.name}</span>
            </div>
          `)}
        </div>
      </div>
    </div>
  `
}
```

- [ ] **Step 2: Commit**

```bash
git add web-lit/src/ui/views/dashboard.ts
git commit -m "feat(rewrite): add dashboard view"
```

---

## Task 20: Devices View — `ui/views/devices.ts`

**Files:**
- Create: `web-lit/src/ui/views/devices.ts`
- Reference: `src/pages/devices-page.ts` (593 lines)

- [ ] **Step 1: Create devices view**

Read `src/pages/devices-page.ts` for its template. The old page has 22 @state properties for filters, pagination, create/edit dialogs, etc. In the new model, these go into AppViewState or remain as local render-time variables.

```ts
// web-lit/src/ui/views/devices.ts
import { html, nothing } from 'lit'
import type { AppViewState } from '../app-view-state'
import { loadDevices, deleteDevice } from '../controllers/devices'
import { navigate } from '../../lib/navigate'

export function renderDevices(state: AppViewState) {
  // Trigger load on first render
  if (state.devices.length === 0 && !state.devicesLoading) {
    loadDevices(state).catch(console.error)
  }

  return html`
    <div class="devices-page">
      <div class="page-header">
        <h2 class="page-title">Devices</h2>
        <button class="btn btn-primary" @click=${() => navigate('/devices/new')}>
          Add Device
        </button>
      </div>

      <div class="devices-filters">
        <input type="text" class="form-field" placeholder="Search devices..."
          .value=${state.devicesParams.search || ''}
          @input=${(e: Event) => {
            state.devicesParams = { ...state.devicesParams, search: (e.target as HTMLInputElement).value }
            loadDevices(state).catch(console.error)
          }}
        />
      </div>

      ${state.devicesLoading
        ? html`<div class="page-loading"><skeleton></skeleton></div>`
        : state.devices.length === 0
          ? html`<div class="page-empty">No devices found</div>`
          : html`
            <div class="device-grid">
              ${state.devices.map(device => html`
                <device-card
                  .device=${device}
                  @click=${() => navigate(`/devices/${device.id}`)}
                ></device-card>
              `)}
            </div>
            ${state.devicesTotalPages > 1 ? html`
              <div class="pagination">
                <button class="btn btn-sm" ?disabled=${state.devicesPage <= 1}
                  @click=${() => loadDevices(state, { page: state.devicesPage - 1 })}>
                  Previous
                </button>
                <span class="pagination-info">Page ${state.devicesPage} of ${state.devicesTotalPages}</span>
                <button class="btn btn-sm" ?disabled=${state.devicesPage >= state.devicesTotalPages}
                  @click=${() => loadDevices(state, { page: state.devicesPage + 1 })}>
                  Next
                </button>
              </div>
            ` : nothing}
          `
      }
    </div>
  `
}
```

- [ ] **Step 2: Commit**

```bash
git add web-lit/src/ui/views/devices.ts
git commit -m "feat(rewrite): add devices view with filters and pagination"
```

---

## Task 21: Device Detail View — `ui/views/device-detail.ts`

**Files:**
- Create: `web-lit/src/ui/views/device-detail.ts`
- Reference: `src/pages/device-detail-page.ts` (794 lines — the most complex page)

- [ ] **Step 1: Create device detail view**

Read the existing device-detail-page.ts. It has tabs for properties, commands, alarms, events, monitoring. Extract the template structure.

```ts
// web-lit/src/ui/views/device-detail.ts
import { html, nothing } from 'lit'
import type { AppViewState } from '../app-view-state'
import { loadDevice } from '../controllers/devices'
import { navigate } from '../../lib/navigate'

type DeviceTab = 'properties' | 'commands' | 'alarms' | 'monitoring'

let activeTab: DeviceTab = 'properties'

export function renderDeviceDetail(state: AppViewState) {
  const id = state.routeParams['id']
  if (!id) {
    navigate('/devices')
    return nothing
  }

  // Load device if not current or wrong ID
  if (!state.currentDevice || state.currentDevice.id !== id) {
    if (!state.deviceDetailLoading) {
      loadDevice(state, id).catch(console.error)
    }
    return html`<div class="page-loading"><skeleton></skeleton></div>`
  }

  const device = state.currentDevice

  return html`
    <div class="device-detail-page">
      <div class="page-header">
        <button class="btn btn-ghost" @click=${() => navigate('/devices')}>← Back</button>
        <h2 class="page-title">${device.displayName || device.name}</h2>
        <span class="status-dot status-${device.status || 'unknown'}"></span>
      </div>

      <div class="device-tabs">
        ${(['properties', 'commands', 'alarms', 'monitoring'] as DeviceTab[]).map(tab => html`
          <button class="tab ${activeTab === tab ? 'active' : ''}"
            @click=${() => { activeTab = tab }}>
            ${tab.charAt(0).toUpperCase() + tab.slice(1)}
          </button>
        `)}
      </div>

      <div class="device-tab-content">
        ${activeTab === 'properties' ? html`
          <div class="properties-grid">
            ${(device.properties || []).map(prop => html`
              <div class="property-card">
                <div class="property-name">${prop.displayName || prop.name}</div>
                <div class="property-value">${prop.currentValue || prop.value || '—'} ${prop.unit || ''}</div>
              </div>
            `)}
            ${!device.properties?.length ? html`<p class="muted">No properties</p>` : nothing}
          </div>
        ` : nothing}
        ${activeTab === 'commands' ? html`
          <div class="commands-list">
            <p class="muted">Commands view — implement with device-card command execution</p>
          </div>
        ` : nothing}
        ${activeTab === 'alarms' ? html`
          <div class="alarms-list">
            <p class="muted">Device alarms — connect to alarms controller</p>
          </div>
        ` : nothing}
        ${activeTab === 'monitoring' ? html`
          <div class="monitoring-view">
            <p class="muted">Device monitoring — connect to performance charts</p>
          </div>
        ` : nothing}
      </div>
    </div>
  `
}
```

- [ ] **Step 2: Commit**

```bash
git add web-lit/src/ui/views/device-detail.ts
git commit -m "feat(rewrite): add device detail view with tabs"
```

---

## Task 22: Alarms View — `ui/views/alarms.ts`

**Files:**
- Create: `web-lit/src/ui/views/alarms.ts`
- Reference: `src/pages/alarms-page.ts` (239 lines)

- [ ] **Step 1: Create alarms view**

```ts
// web-lit/src/ui/views/alarms.ts
import { html, nothing } from 'lit'
import type { AppViewState } from '../app-view-state'
import { loadAlarms, acknowledgeAlarm, resolveAlarm } from '../controllers/alarms'

export function renderAlarms(state: AppViewState) {
  if (state.alarms.length === 0 && !state.alarmsLoading) {
    loadAlarms(state).catch(console.error)
  }

  return html`
    <div class="alarms-page">
      <h2 class="page-title">Alarms</h2>

      ${state.alarmsLoading
        ? html`<div class="page-loading"><skeleton></skeleton></div>`
        : state.alarms.length === 0
          ? html`<div class="page-empty">No alarms</div>`
          : html`
            <div class="alarm-list">
              ${state.alarms.map(alarm => html`
                <div class="alarm-item alarm-${alarm.alarmLevel.toLowerCase()}">
                  <div class="alarm-header">
                    <span class="alarm-level">${alarm.alarmLevel}</span>
                    <span class="alarm-device">${alarm.deviceName || alarm.deviceId}</span>
                    <span class="alarm-status">${alarm.status}</span>
                  </div>
                  <div class="alarm-message">${alarm.message}</div>
                  <div class="alarm-time">${alarm.alarmTime}</div>
                  <div class="alarm-actions">
                    ${!alarm.isAcknowledged ? html`
                      <button class="btn btn-sm" @click=${() => acknowledgeAlarm(state, alarm.id)}>Acknowledge</button>
                    ` : nothing}
                    ${!alarm.isResolved ? html`
                      <button class="btn btn-sm" @click=${() => resolveAlarm(state, alarm.id)}>Resolve</button>
                    ` : nothing}
                  </div>
                </div>
              `)}
            </div>
            ${state.alarmsTotalPages > 1 ? html`
              <div class="pagination">
                <button class="btn btn-sm" ?disabled=${state.alarmsPage <= 1}
                  @click=${() => loadAlarms(state, { page: state.alarmsPage - 1 })}>Previous</button>
                <span>Page ${state.alarmsPage} of ${state.alarmsTotalPages}</span>
                <button class="btn btn-sm" ?disabled=${state.alarmsPage >= state.alarmsTotalPages}
                  @click=${() => loadAlarms(state, { page: state.alarmsPage + 1 })}>Next</button>
              </div>
            ` : nothing}
          `
      }
    </div>
  `
}
```

- [ ] **Step 2: Commit**

```bash
git add web-lit/src/ui/views/alarms.ts
git commit -m "feat(rewrite): add alarms view"
```

---

## Task 23: Monitoring View — `ui/views/monitoring.ts`

**Files:**
- Create: `web-lit/src/ui/views/monitoring.ts`
- Reference: `src/pages/monitoring-page.ts` (205 lines)

- [ ] **Step 1: Create monitoring view**

```ts
// web-lit/src/ui/views/monitoring.ts
import { html, nothing } from 'lit'
import type { AppViewState } from '../app-view-state'
import { loadMonitoringData } from '../controllers/monitoring'

export function renderMonitoring(state: AppViewState) {
  if (!state.monitoringLoading && !state.dashboardData) {
    loadMonitoringData(state).catch(console.error)
  }

  const metrics = state.dashboardData?.systemMetrics

  return html`
    <div class="monitoring-page">
      <h2 class="page-title">System Monitoring</h2>

      ${state.monitoringLoading
        ? html`<div class="page-loading"><skeleton></skeleton></div>`
        : metrics ? html`
          <div class="metrics-grid">
            <div class="metric-card">
              <div class="metric-label">CPU Usage</div>
              <div class="metric-value">${metrics.cpu}%</div>
              <div class="metric-bar"><div class="metric-fill" style="width: ${metrics.cpu}%"></div></div>
            </div>
            <div class="metric-card">
              <div class="metric-label">Memory</div>
              <div class="metric-value">${metrics.memory}%</div>
              <div class="metric-bar"><div class="metric-fill" style="width: ${metrics.memory}%"></div></div>
            </div>
            <div class="metric-card">
              <div class="metric-label">Disk</div>
              <div class="metric-value">${metrics.disk}%</div>
              <div class="metric-bar"><div class="metric-fill" style="width: ${metrics.disk}%"></div></div>
            </div>
            <div class="metric-card">
              <div class="metric-label">Network</div>
              <div class="metric-value">↓${metrics.network.inbound} ↑${metrics.network.outbound}</div>
            </div>
          </div>
        ` : html`<div class="page-empty">No monitoring data available</div>`
      }
    </div>
  `
}
```

- [ ] **Step 2: Commit**

```bash
git add web-lit/src/ui/views/monitoring.ts
git commit -m "feat(rewrite): add monitoring view"
```

---

## Task 24: Agent View — `ui/views/agent.ts`

**Files:**
- Create: `web-lit/src/ui/views/agent.ts`
- Reference: `src/pages/agent-page.ts` (179 lines)

- [ ] **Step 1: Create agent view**

```ts
// web-lit/src/ui/views/agent.ts
import { html, nothing } from 'lit'
import type { AppViewState } from '../app-view-state'
import { sendAgentMessage, clearChat } from '../controllers/agent'

export function renderAgent(state: AppViewState) {
  return html`
    <div class="agent-page">
      <div class="agent-header">
        <h2 class="page-title">AI Agent</h2>
        ${state.chatMessages.length > 0 ? html`
          <button class="btn btn-ghost btn-sm" @click=${() => clearChat(state)}>Clear Chat</button>
        ` : nothing}
      </div>

      <div class="agent-chat">
        <chat-thread .messages=${state.chatMessages}></chat-thread>

        ${state.isStreaming ? html`
          <streaming-message .content=${state.streamingContent}></streaming-message>
        ` : nothing}
      </div>

      <chat-input
        .disabled=${state.isStreaming}
        @send=${(e: CustomEvent) => sendAgentMessage(state, e.detail.message)}
      ></chat-input>

      ${state.chatMessages.some(m => m.surfaces && m.surfaces.size > 0) ? html`
        <div class="a2ui-surfaces">
          ${state.chatMessages.filter(m => m.surfaces).map(msg => html`
            ${Array.from(msg.surfaces!.values()).map(surface => html`
              <a2ui-surface .surface=${surface}></a2ui-surface>
            `)}
          `)}
        </div>
      ` : nothing}
    </div>
  `
}
```

- [ ] **Step 2: Commit**

```bash
git add web-lit/src/ui/views/agent.ts
git commit -m "feat(rewrite): add agent view with chat and A2UI surfaces"
```

---

## Task 25: Settings View — `ui/views/settings.ts`

**Files:**
- Create: `web-lit/src/ui/views/settings.ts`
- Reference: `src/pages/settings-page.ts` (255 lines)

- [ ] **Step 1: Create settings view**

```ts
// web-lit/src/ui/views/settings.ts
import { html, nothing } from 'lit'
import type { AppViewState } from '../app-view-state'
import { loadProfile, updateProfile, changePassword } from '../controllers/auth'

export function renderSettings(state: AppViewState) {
  if (!state.user && !state.settingsLoading) {
    loadProfile(state).catch(console.error)
  }

  let passwordError = ''
  let passwordSuccess = ''

  return html`
    <div class="settings-page">
      <h2 class="page-title">Settings</h2>

      ${state.user ? html`
        <div class="settings-section card">
          <h3>Profile</h3>
          <div class="form-group">
            <label>Name</label>
            <input type="text" class="form-field" .value=${state.user.name} readonly />
          </div>
          <div class="form-group">
            <label>Email</label>
            <input type="email" class="form-field" .value=${state.user.email || ''} readonly />
          </div>
        </div>

        <div class="settings-section card">
          <h3>Change Password</h3>
          ${passwordError ? html`<div class="callout callout-error">${passwordError}</div>` : nothing}
          ${passwordSuccess ? html`<div class="callout callout-success">${passwordSuccess}</div>` : nothing}
          <form @submit=${async (e: Event) => {
            e.preventDefault()
            const form = e.target as HTMLFormElement
            const oldPw = (form.querySelector('#old-password') as HTMLInputElement).value
            const newPw = (form.querySelector('#new-password') as HTMLInputElement).value
            try {
              await changePassword(state, oldPw, newPw)
              passwordSuccess = 'Password changed'
              passwordError = ''
            } catch (err) {
              passwordError = err instanceof Error ? err.message : 'Failed'
              passwordSuccess = ''
            }
          }}>
            <div class="form-group">
              <label for="old-password">Current Password</label>
              <input id="old-password" type="password" class="form-field" required />
            </div>
            <div class="form-group">
              <label for="new-password">New Password</label>
              <input id="new-password" type="password" class="form-field" required />
            </div>
            <button type="submit" class="btn btn-primary">Change Password</button>
          </form>
        </div>
      ` : html`<div class="page-loading"><skeleton></skeleton></div>`}
    </div>
  `
}
```

- [ ] **Step 2: Commit**

```bash
git add web-lit/src/ui/views/settings.ts
git commit -m "feat(rewrite): add settings view"
```

---

## Task 26: Tags View — `ui/views/tags.ts`

**Files:**
- Create: `web-lit/src/ui/views/tags.ts`
- Reference: `src/pages/tags-page.ts` (229 lines)

- [ ] **Step 1: Create tags view**

```ts
// web-lit/src/ui/views/tags.ts
import { html, nothing } from 'lit'
import type { AppViewState } from '../app-view-state'
import { loadTags, createTag, deleteTag } from '../controllers/tags'

export function renderTags(state: AppViewState) {
  if (state.tags.length === 0 && !state.tagsLoading) {
    loadTags(state).catch(console.error)
  }

  let newTagName = ''
  let showCreate = false

  return html`
    <div class="tags-page">
      <div class="page-header">
        <h2 class="page-title">Tags</h2>
        <button class="btn btn-primary" @click=${() => { showCreate = !showCreate }}>
          ${showCreate ? 'Cancel' : 'Add Tag'}
        </button>
      </div>

      ${showCreate ? html`
        <div class="card create-tag-form">
          <form @submit=${(e: Event) => {
            e.preventDefault()
            if (newTagName.trim()) {
              createTag(state, newTagName.trim())
              newTagName = ''
              showCreate = false
            }
          }}>
            <input type="text" class="form-field" placeholder="Tag name"
              .value=${newTagName}
              @input=${(e: Event) => { newTagName = (e.target as HTMLInputElement).value }}
            />
            <button type="submit" class="btn btn-primary">Create</button>
          </form>
        </div>
      ` : nothing}

      ${state.tagsLoading
        ? html`<div class="page-loading"><skeleton></skeleton></div>`
        : state.tags.length === 0
          ? html`<div class="page-empty">No tags</div>`
          : html`
            <div class="tag-list">
              ${state.tags.map(tag => html`
                <div class="tag-item">
                  <span class="tag-name" style=${tag.color ? `color: ${tag.color}` : ''}>${tag.name}</span>
                  <span class="tag-type muted">${tag.type}</span>
                  <button class="btn btn-ghost btn-sm" @click=${() => deleteTag(state, tag.id)}>Delete</button>
                </div>
              `)}
            </div>
          `
      }
    </div>
  `
}
```

- [ ] **Step 2: Commit**

```bash
git add web-lit/src/ui/views/tags.ts
git commit -m "feat(rewrite): add tags view"
```

---

## Task 27: Templates View — `ui/views/templates.ts`

**Files:**
- Create: `web-lit/src/ui/views/templates.ts`
- Reference: `src/pages/templates-page.ts` (195 lines)

- [ ] **Step 1: Create templates view**

```ts
// web-lit/src/ui/views/templates.ts
import { html, nothing } from 'lit'
import type { AppViewState } from '../app-view-state'
import { apiGet } from '../api-client'
import type { Template } from '../types'

async function loadTemplates(host: AppViewState): Promise<void> {
  host.templatesLoading = true
  try {
    const res = await apiGet<{ items: Template[]; totalPages: number; page: number }>('device-templates', host.templatesParams as Record<string, unknown>)
    if (res.result) {
      host.templates = res.result.items
      host.templatesTotalPages = res.result.totalPages
      host.templatesPage = res.result.page
    }
  } finally {
    host.templatesLoading = false
  }
}

export function renderTemplates(state: AppViewState) {
  if (state.templates.length === 0 && !state.templatesLoading) {
    loadTemplates(state).catch(console.error)
  }

  return html`
    <div class="templates-page">
      <h2 class="page-title">Device Templates</h2>

      ${state.templatesLoading
        ? html`<div class="page-loading"><skeleton></skeleton></div>`
        : state.templates.length === 0
          ? html`<div class="page-empty">No templates</div>`
          : html`
            <div class="template-grid">
              ${state.templates.map(t => html`
                <template-card .template=${t}></template-card>
              `)}
            </div>
            ${state.templatesTotalPages > 1 ? html`
              <div class="pagination">
                <button class="btn btn-sm" ?disabled=${state.templatesPage <= 1}
                  @click=${() => { state.templatesParams = { ...state.templatesParams, page: state.templatesPage - 1 }; loadTemplates(state) }}>
                  Previous
                </button>
                <span>Page ${state.templatesPage} of ${state.templatesTotalPages}</span>
                <button class="btn btn-sm" ?disabled=${state.templatesPage >= state.templatesTotalPages}
                  @click=${() => { state.templatesParams = { ...state.templatesParams, page: state.templatesPage + 1 }; loadTemplates(state) }}>
                  Next
                </button>
              </div>
            ` : nothing}
          `
      }
    </div>
  `
}
```

- [ ] **Step 2: Commit**

```bash
git add web-lit/src/ui/views/templates.ts
git commit -m "feat(rewrite): add templates view"
```

---

## Task 28: Marketplace View — `ui/views/marketplace.ts`

**Files:**
- Create: `web-lit/src/ui/views/marketplace.ts`
- Reference: `src/pages/marketplace-page.ts` (448 lines), `src/pages/installed-marketplace-page.ts` (233 lines)

- [ ] **Step 1: Create marketplace view**

The existing marketplace uses mock data only. Keep it simple.

```ts
// web-lit/src/ui/views/marketplace.ts
import { html, nothing } from 'lit'
import type { AppViewState } from '../app-view-state'

export function renderMarketplace(state: AppViewState) {
  return html`
    <div class="marketplace-page">
      <h2 class="page-title">Marketplace</h2>
      <div class="marketplace-tabs">
        <button class="tab ${state.currentRoute === 'marketplace' ? 'active' : ''}">Browse</button>
        <button class="tab ${state.currentRoute === 'marketplace-installed' ? 'active' : ''}">Installed</button>
      </div>
      <div class="marketplace-content">
        <div class="page-empty">Marketplace coming soon</div>
      </div>
    </div>
  `
}
```

- [ ] **Step 2: Commit**

```bash
git add web-lit/src/ui/views/marketplace.ts
git commit -m "feat(rewrite): add marketplace view (stub)"
```

---

## Task 29: Remaining Shell Wiring — update app-render.ts imports

**Files:**
- Modify: `web-lit/src/ui/app-render.ts`

- [ ] **Step 1: Verify all view imports resolve**

By this point all 13 views exist. Verify `app-render.ts` imports compile cleanly.

Run: `cd web-lit && npx tsc --noEmit src/ui/app-render.ts`
Expected: Should compile cleanly now that all views, components, and types exist.

- [ ] **Step 2: Fix any import errors**

If any view has wrong imports, fix them now.

- [ ] **Step 3: Commit**

```bash
git add web-lit/src/ui/app-render.ts
git commit -m "fix(rewrite): fix app-render.ts imports for all views"
```

---

## Task 30: CSS Split — layout.css refactor + layout.mobile.css + iot.css

**Files:**
- Modify: `web-lit/src/styles/layout.css` (refactor to shell only)
- Create: `web-lit/src/styles/layout.mobile.css`
- Create: `web-lit/src/styles/iot.css`
- Modify: `web-lit/src/styles/components.css` (remove page styles)

- [ ] **Step 1: Refactor layout.css to shell-only**

Read `layout.css`. Keep only the Shell Layout (lines 4-79), Topbar (lines 80-309), Navigation Sidebar (lines 310-862), Content Area (lines 863-942), and Grid Utilities (lines 943-990) sections. Remove the Responsive - Tablet section (lines 991-1044) — move it to layout.mobile.css.

- [ ] **Step 2: Create layout.mobile.css**

Extract responsive rules from layout.css (lines 991-1044) and any mobile-specific rules from components.css. Add mobile-specific overrides:

```css
/* web-lit/src/styles/layout.mobile.css */
/* Mobile responsive rules for shell layout */

@media (max-width: 768px) {
  .app-shell {
    grid-template-columns: 0 1fr;
  }

  .sidebar {
    transform: translateX(-100%);
    transition: transform 0.2s ease;
  }

  .sidebar.open {
    transform: translateX(0);
    z-index: 100;
  }

  .topbar {
    padding: 0 12px;
  }

  .app-content {
    padding: 12px;
  }

  .device-grid,
  .template-grid,
  .stats-grid {
    grid-template-columns: 1fr;
  }

  .dashboard-grid {
    grid-template-columns: 1fr;
  }
}
```

- [ ] **Step 3: Create iot.css**

Extract IoT-specific sections from components.css: device-card, device-detail-page, devices-page, alarms-page, dashboard-page, monitoring-page, performance-chart, performance-metrics-card, performance-alerts, trace-records, device-status-card, device-table, device-info-form, command-execute-dialog, create-device-wizard, property-chart-dialog, a2ui-device-card, control-panel, data-chart, progress-indicator, real-time-toggle.

These are the sections at lines: 733-788, 1453-1999, 2013-2319, 2817-2903, 2903-3207, 3207-3592, 3592-4397, 4397-4960, 6215-6469, 605-733, 788-876.

- [ ] **Step 4: Clean components.css**

Remove page-specific sections from components.css. Keep only: Cards, Stats, Labels & Pills, Status Dot, Buttons, Form Fields, Utilities, Callouts, Code Blocks, skeleton, tag-filter, tag-selector, template-card, template-preview, workspace-picker, chat-input, chat-thread, message-group, streaming-message, a2ui-component, a2ui-surface, a2ui-button, a2ui-card, a2ui-column, a2ui-divider, a2ui-text, confirmation-dialog, logo-icon.

Remove: all `*-page` sections (agent-page, alarms-page, dashboard-page, device-detail-page, devices-page, home-page, installed-marketplace-page, marketplace-page, monitoring-page, register-page, settings-page, signin-page, tags-page, templates-page, base-page, tinyiothub-app).

- [ ] **Step 5: Commit**

```bash
git add web-lit/src/styles/layout.css web-lit/src/styles/layout.mobile.css web-lit/src/styles/iot.css web-lit/src/styles/components.css
git commit -m "refactor(rewrite): split CSS into layout, layout.mobile, components, iot"
```

---

## Task 31: Create styles.css entry + update main.ts

**Files:**
- Create: `web-lit/src/styles.css`
- Modify: `web-lit/src/main.ts`

- [ ] **Step 1: Create styles.css CSS entry**

```css
/* web-lit/src/styles.css */
@import './styles/base.css';
@import './styles/layout.css';
@import './styles/layout.mobile.css';
@import './styles/components.css';
@import './styles/iot.css';
```

- [ ] **Step 2: Update main.ts**

```ts
// web-lit/src/main.ts
import './styles.css'
import './ui/app'

const app = document.createElement('tinyiothub-app')
document.querySelector('#app')!.appendChild(app)
```

- [ ] **Step 3: Commit**

```bash
git add web-lit/src/styles.css web-lit/src/main.ts
git commit -m "feat(rewrite): add styles.css entry and update main.ts"
```

---

## Task 32: Move reusable components to ui/components/

**Files:**
- Move: `src/components/device-card.ts` → `src/ui/components/device-card.ts`
- Move: `src/components/create-device-wizard.ts` → `src/ui/components/device-form.ts`
- Move: `src/components/agent/chat-input.ts` → `src/ui/components/chat-input.ts`
- Move: `src/components/agent/chat-thread.ts` → `src/ui/components/chat-thread.ts`
- Move: `src/components/agent/message-group.ts` → `src/ui/components/message-group.ts`
- Move: `src/components/agent/streaming-message.ts` → `src/ui/components/streaming-message.ts`
- Move: `src/components/skeleton.ts` → `src/ui/components/skeleton.ts`
- Move: `src/components/tag-filter.ts` → `src/ui/components/tag-filter.ts`
- Move: `src/components/tag-selector.ts` → `src/ui/components/tag-selector.ts`
- Move: `src/components/template-card.ts` → `src/ui/components/template-card.ts`
- Move: `src/components/template-preview.ts` → `src/ui/components/template-preview.ts`
- Move: `src/components/logo-icon.ts` → `src/ui/components/logo-icon.ts`
- Move: entire `src/components/agent/a2ui/` → `src/ui/components/a2ui/`
- Move: `src/components/monitoring/*.ts` → `src/ui/components/monitoring/`

For each moved component:
1. Update import paths (remove nanostore imports if any)
2. For components that subscribed to nanostores (app-sidebar, app-header, workspace-picker), they are replaced by the new sidebar.ts and topbar.ts — don't move them, leave for deletion
3. Update relative imports to use new paths

- [ ] **Step 1: Move non-nanostore components**

Copy each file to the new location. Update import paths from `../../stores/` to `../` (for types) and from `../../services/` to `../controllers/`.

- [ ] **Step 2: Move A2UI components**

Copy `src/components/agent/a2ui/` directory to `src/ui/components/a2ui/`. Update import paths.

- [ ] **Step 3: Move monitoring components**

Copy `src/components/monitoring/*.ts` to `src/ui/components/monitoring/`.

- [ ] **Step 4: Update all import references**

Search for any remaining imports referencing old `src/components/` paths and update them.

- [ ] **Step 5: Commit**

```bash
git add web-lit/src/ui/components/
git commit -m "refactor(rewrite): move reusable components to ui/components/"
```

---

## Task 33: Update vite.config.ts and package.json

**Files:**
- Modify: `web-lit/vite.config.ts`
- Modify: `web-lit/package.json`

- [ ] **Step 1: Update vite.config.ts manual chunks**

Remove `@lit-labs/router` and `ky` from the manual chunks config since we're removing those dependencies.

```ts
// In web-lit/vite.config.ts, update manualChunks:
manualChunks: {
  'lit': ['lit'],
  'vendor': ['chart.js', 'i18next', 'dompurify', 'marked'],
},
```

- [ ] **Step 2: Remove unused dependencies from package.json**

```bash
cd web-lit && npm uninstall @lit-labs/router @nanostores/react ky nanostores
```

- [ ] **Step 3: Commit**

```bash
git add web-lit/package.json web-lit/vite.config.ts web-lit/package-lock.json
git commit -m "chore(rewrite): remove nanostores, @lit-labs/router, ky dependencies"
```

---

## Task 34: Delete old code

**Files to delete:**
- `web-lit/src/stores/` (entire directory — 5 files)
- `web-lit/src/pages/` (entire directory — 14 files)
- `web-lit/src/services/` (entire directory — 11 files)
- `web-lit/src/types/` (entire directory — 11 files, replaced by ui/types.ts)
- `web-lit/src/views/base-page.ts` (dead code)
- `web-lit/src/components/app-header.ts` (replaced by ui/components/topbar.ts)
- `web-lit/src/components/app-sidebar.ts` (replaced by ui/components/sidebar.ts)
- `web-lit/src/components/workspace-picker.ts` (replaced by inline in topbar.ts)
- `web-lit/src/app.ts` (replaced by ui/app.ts)

- [ ] **Step 1: Delete stores/**

```bash
rm -rf web-lit/src/stores/
```

- [ ] **Step 2: Delete pages/**

```bash
rm -rf web-lit/src/pages/
```

- [ ] **Step 3: Delete services/**

```bash
rm -rf web-lit/src/services/
```

- [ ] **Step 4: Delete old types/**

```bash
rm -rf web-lit/src/types/
```

- [ ] **Step 5: Delete old root app.ts and views/base-page.ts**

```bash
rm web-lit/src/app.ts web-lit/src/views/base-page.ts
```

- [ ] **Step 6: Delete replaced components**

```bash
rm web-lit/src/components/app-header.ts web-lit/src/components/app-sidebar.ts web-lit/src/components/workspace-picker.ts
```

- [ ] **Step 7: Delete old lib/api-client.ts (replaced by ui/api-client.ts)**

Note: Keep lib/case-converter.ts, lib/config.ts, lib/navigate.ts, lib/local-storage.ts — they're still used.

```bash
rm web-lit/src/lib/api-client.ts
```

- [ ] **Step 8: Commit**

```bash
git add -A
git commit -m "refactor(rewrite): delete old stores, pages, services, types, and replaced files"
```

---

## Task 35: Fix remaining imports and verify build

**Files:**
- Fix any remaining broken imports across the codebase

- [ ] **Step 1: Run TypeScript check**

```bash
cd web-lit && npx tsc --noEmit
```

- [ ] **Step 2: Fix all reported errors**

Common issues to expect:
- Imports still referencing old `../stores/` paths
- Imports still referencing old `../services/` paths
- Imports still referencing old `../types/` paths
- Missing component imports in views

- [ ] **Step 3: Run Vite build**

```bash
cd web-lit && npm run build
```

- [ ] **Step 4: Fix any build errors**

- [ ] **Step 5: Final commit**

```bash
git add -A
git commit -m "fix(rewrite): resolve all import errors, build passes"
```

---

## Spec Coverage Checklist

| Spec Requirement | Task |
|-----------------|------|
| Single root `<tinyiothub-app>` with @state | Task 6 |
| AppViewState type | Task 4 |
| Declarative route table (14 routes) | Task 3 |
| Pure function views (13 views) | Tasks 18-28 |
| Controllers (7 controllers) | Tasks 10-16 |
| CSS split (5 files) | Tasks 30-31 |
| API client decoupled from nanostores | Task 2 |
| Theme management | Task 5 |
| Icon library | Task 5 |
| Lifecycle delegation | Task 7 |
| Component migration | Task 32 |
| Delete stores/, pages/, services/, old types | Task 34 |
| Remove nanostores, @lit-labs/router, ky deps | Task 33 |
| Delete dead code (base-page.ts) | Task 34 |
| Merge duplicate $sidebarCollapsed/$navCollapsed | Task 6 (single navCollapsed) |
| Agent uses ApiClient (not raw fetch bypass) | Task 15 (SSE still needs raw fetch for streaming, but auth token comes from shared state) |
| Consolidated types (no duplicates) | Task 1 |
| Build passes | Task 35 |
