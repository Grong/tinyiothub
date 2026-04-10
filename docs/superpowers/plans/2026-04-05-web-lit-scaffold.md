# Web-Lit Scaffold Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Create the `web-lit/` project scaffold with Vite + Lit, copy CSS system, set up routing, create placeholder pages, and extract/refactor the API client and services.

**Architecture:** Greenfield Lit Web Components project at `web-lit/` parallel to existing `web/`. Services are pure async functions copied from `web/service/` with React Query wrappers removed. API client adapted for Vite env (using `import.meta.env` instead of `process.env`).

**Tech Stack:** Lit 3.x, Vite 8.x, Nanostores, @lit-labs/router, ky, i18next, Vitest, Playwright

---

## File Structure

```
web-lit/
├── src/
│   ├── components/
│   │   └── shell/
│   │       └── shell.ts
│   ├── pages/
│   │   ├── home-page.ts
│   │   ├── signin-page.ts
│   │   ├── register-page.ts
│   │   ├── dashboard-page.ts
│   │   ├── devices-page.ts
│   │   ├── device-detail-page.ts
│   │   ├── alarms-page.ts
│   │   ├── monitoring-page.ts
│   │   ├── settings-page.ts
│   │   ├── tags-page.ts
│   │   ├── templates-page.ts
│   │   ├── marketplace-page.ts
│   │   └── installed-marketplace-page.ts
│   ├── services/
│   │   ├── auth.ts
│   │   ├── devices.ts
│   │   ├── alarms.ts
│   │   ├── dashboard.ts
│   │   ├── events.ts
│   │   ├── templates.ts
│   │   ├── tenant.ts
│   │   └── users.ts
│   ├── lib/
│   │   ├── api-client.ts
│   │   ├── case-converter.ts
│   │   └── config.ts
│   ├── stores/
│   │   ├── auth-store.ts
│   │   └── app-store.ts
│   ├── router/
│   │   └── index.ts
│   ├── i18n/
│   │   └── [copied from web/i18n/]
│   ├── types/
│   │   └── [copied from web/types/]
│   ├── styles/
│   │   ├── base.css
│   │   ├── layout.css
│   │   └── components.css
│   ├── app.ts
│   └── main.ts
├── index.html
├── vite.config.ts
├── tsconfig.json
└── package.json
```

---

## Phase 0: Project Scaffold

### Task 1: Initialize `web-lit/` Project

**Files:**
- Create: `web-lit/package.json`
- Create: `web-lit/vite.config.ts`
- Create: `web-lit/tsconfig.json`
- Create: `web-lit/index.html`

- [ ] **Step 1: Write `package.json`**

```json
{
  "name": "web-lit",
  "version": "0.1.0",
  "private": true,
  "type": "module",
  "scripts": {
    "dev": "vite",
    "build": "tsc && vite build",
    "preview": "vite preview",
    "test": "vitest run",
    "test:watch": "vitest",
    "test:ui": "vitest --ui",
    "test:e2e": "playwright test"
  },
  "dependencies": {
    "lit": "^3.3.0",
    "@lit-labs/router": "^0.1.3",
    "nanostores": "^0.10.3",
    "@nanostores/react": "^0.7.3",
    "ky": "^1.7.0",
    "i18next": "^23.16.0",
    "chart.js": "^4.4.6"
  },
  "devDependencies": {
    "typescript": "^5.6.0",
    "vite": "^8.0.0",
    "@vitest/browser-playwright": "^4.1.0",
    "vitest": "^4.1.0",
    "@playwright/test": "^1.48.0",
    "jsdom": "^25.0.0"
  }
}
```

- [ ] **Step 2: Write `vite.config.ts`**

```typescript
import { defineConfig } from 'vite'
import path from 'node:path'
import { fileURLToPath } from 'node:url'

const here = path.dirname(fileURLToPath(import.meta.url))

export default defineConfig({
  resolve: {
    alias: {
      '@': path.resolve(here, './src'),
    },
  },
  server: {
    host: true,
    port: 5173,
    strictPort: true,
    proxy: {
      '/api': {
        target: process.env.VITE_API_TARGET || 'http://localhost:8080',
        changeOrigin: true,
      },
    },
  },
  build: {
    outDir: 'dist',
    emptyOutDir: true,
    sourcemap: true,
    rollupOptions: {
      output: {
        manualChunks: {
          lit: ['lit', '@lit-labs/router'],
          vendor: ['ky', 'chart.js', 'i18next'],
        },
      },
    },
  },
})
```

- [ ] **Step 3: Write `tsconfig.json`**

```json
{
  "compilerOptions": {
    "target": "ES2022",
    "module": "ESNext",
    "moduleResolution": "bundler",
    "lib": ["ES2022", "DOM", "DOM.Iterable"],
    "strict": true,
    "noEmit": true,
    "esModuleInterop": true,
    "skipLibCheck": true,
    "forceConsistentCasingInFileNames": true,
    "resolveJsonModule": true,
    "isolatedModules": true,
    "useDefineForClassFields": false,
    "experimentalDecorators": true,
    "paths": {
      "@/*": ["./src/*"]
    }
  },
  "include": ["src"],
  "exclude": ["node_modules", "dist"]
}
```

- [ ] **Step 4: Write `index.html`**

```html
<!DOCTYPE html>
<html lang="en">
<head>
  <meta charset="UTF-8" />
  <meta name="viewport" content="width=device-width, initial-scale=1.0" />
  <title>TinyIoTHub</title>
  <link rel="icon" type="image/svg+xml" href="/logo.svg" />
</head>
<body>
  <script type="module" src="/src/main.ts"></script>
</body>
</html>
```

- [ ] **Step 5: Install dependencies**

Run: `cd /Users/chenguorong/code/my/tinyiothub/web-lit && pnpm install`
Expected: Dependencies installed, `node_modules/` created

- [ ] **Step 6: Commit**

```bash
git add web-lit/
git commit -m "feat(web-lit): initialize project scaffold with Vite + Lit + TypeScript"
```

---

### Task 2: Copy CSS System from OpenClaw

**Files:**
- Create: `web-lit/src/styles/base.css`
- Create: `web-lit/src/styles/layout.css`
- Create: `web-lit/src/styles/components.css`

**Context:** OpenClaw CSS at `/Users/chenguorong/code/github/openclaw/ui/src/styles/` is ~213KB total. Copy only what's needed for shell/layout (~50KB gzipped target). Do NOT copy `chat.css`, `dreams.css`, `usage.css`, `config.css`, or `layout.mobile.css` — these are chat-specific or redundant.

- [ ] **Step 1: Create `web-lit/src/styles/base.css`**

Copy content from `/Users/chenguorong/code/github/openclaw/ui/src/styles/base.css` — this contains CSS variables (color palette, spacing tokens).

Add this header comment:
```css
/**
 * TinyIoTHub Design System - Base Styles
 * Adapted from OpenClaw UI (/Users/chenguorong/code/github/openclaw/ui/src/styles/base.css)
 * DO NOT copy chat.css, dreams.css, or config.css — those are OpenClaw-specific
 */
```

Then copy the full content of `base.css` from OpenClaw.

- [ ] **Step 2: Create `web-lit/src/styles/layout.css`**

Copy content from `/Users/chenguorong/code/github/openclaw/ui/src/styles/layout.css` — this contains the shell grid layout, sidebar, topbar, and responsive behavior.

Add same header comment.

- [ ] **Step 3: Create `web-lit/src/styles/components.css`**

Copy the first ~300 lines from `/Users/chenguorong/code/github/openclaw/ui/src/styles/components.css` — select only button, input, card, badge, modal base styles. DO NOT copy the full 82KB file. Copy selectively:

```
# Extract shell-relevant component styles:
# - .button, .btn variants (search for "^.button" and "^.btn")
# - .input, .text-field
# - .card
# - .badge
# - .modal, .dialog
# - .spinner
# Skip: chat-specific, editor-specific, markdown-specific styles
```

**Target size: <50KB gzipped for all CSS combined.**

- [ ] **Step 4: Commit**

```bash
git add web-lit/src/styles/
git commit -m "feat(web-lit): copy CSS system from OpenClaw (base, layout, components)"
```

---

### Task 3: Set Up Entry Points and Router

**Files:**
- Create: `web-lit/src/main.ts`
- Create: `web-lit/src/app.ts`
- Create: `web-lit/src/router/index.ts`

- [ ] **Step 1: Write `web-lit/src/main.ts`**

```typescript
import './styles/base.css'
import './styles/layout.css'
import './styles/components.css'
import { App } from './app'

// Mount the app
const root = document.getElementById('app')
if (root) {
  const app = new App()
  root.appendChild(app)
}
```

- [ ] **Step 2: Write `web-lit/src/app.ts`**

```typescript
import { LitElement, html, css } from 'lit'
import { customElement, state } from 'lit/decorators.js'
import { router } from './router'

@customElement('tinyiothub-app')
export class App extends LitElement {
  static styles = css`
    :host {
      display: contents;
    }
  `

  @state() private _currentRoute = '/'

  connectedCallback() {
    super.connectedCallback()
    this._currentRoute = window.location.pathname
    window.addEventListener('popstate', () => {
      this._currentRoute = window.location.pathname
    })
  }

  render() {
    return html`<div>Hello TinyIoTHub</div>`
  }
}
```

- [ ] **Step 3: Write `web-lit/src/router/index.ts`**

```typescript
import { html } from 'lit'
import { Route, Router } from '@lit-labs/router'
import '../pages/home-page'
import '../pages/signin-page'
import '../pages/register-page'
import '../pages/dashboard-page'
import '../pages/devices-page'
import '../pages/device-detail-page'
import '../pages/alarms-page'
import '../pages/monitoring-page'
import '../pages/settings-page'
import '../pages/tags-page'
import '../pages/templates-page'
import '../pages/marketplace-page'
import '../pages/installed-marketplace-page'

export const routes: Route[] = [
  { path: '/', component: 'home-page' },
  { path: '/signin', component: 'signin-page' },
  { path: '/tenant/register', component: 'register-page' },
  { path: '/dashboard', component: 'dashboard-page' },
  { path: '/devices', component: 'devices-page' },
  { path: '/device-detail/:id', component: 'device-detail-page' },
  { path: '/alarms', component: 'alarms-page' },
  { path: '/monitoring', component: 'monitoring-page' },
  { path: '/settings', component: 'settings-page' },
  { path: '/tags', component: 'tags-page' },
  { path: '/templates', component: 'templates-page' },
  { path: '/marketplace', component: 'marketplace-page' },
  { path: '/installed-marketplace', component: 'installed-marketplace-page' },
]

export const router = new Router(document.body, routes)

// Helper to navigate
export function navigate(path: string) {
  history.pushState({}, '', path)
  window.dispatchEvent(new PopStateEvent('popstate'))
}
```

- [ ] **Step 4: Commit**

```bash
git add web-lit/src/main.ts web-lit/src/app.ts web-lit/src/router/
git commit -m "feat(web-lit): set up entry points and router"
```

---

### Task 4: Create Placeholder Pages

**Files:**
- Create: `web-lit/src/pages/home-page.ts`
- Create: `web-lit/src/pages/signin-page.ts`
- Create: `web-lit/src/pages/register-page.ts`
- Create: `web-lit/src/pages/dashboard-page.ts`
- Create: `web-lit/src/pages/devices-page.ts`
- Create: `web-lit/src/pages/device-detail-page.ts`
- Create: `web-lit/src/pages/alarms-page.ts`
- Create: `web-lit/src/pages/monitoring-page.ts`
- Create: `web-lit/src/pages/settings-page.ts`
- Create: `web-lit/src/pages/tags-page.ts`
- Create: `web-lit/src/pages/templates-page.ts`
- Create: `web-lit/src/pages/marketplace-page.ts`
- Create: `web-lit/src/pages/installed-marketplace-page.ts`

- [ ] **Step 1: Create all placeholder pages**

Each page should be a minimal Lit component:

```typescript
// Example: home-page.ts
import { LitElement, html, css } from 'lit'
import { customElement } from 'lit/decorators.js'

@customElement('home-page')
export class HomePage extends LitElement {
  static styles = css`
    :host {
      display: block;
      padding: 24px;
    }
  `

  render() {
    return html`<div>Home Page - Placeholder</div>`
  }
}
```

Create all 13 pages with this minimal structure.

- [ ] **Step 2: Verify dev server runs**

Run: `cd web-lit && pnpm dev`
Expected: Vite dev server starts on port 5173 without errors

- [ ] **Step 3: Commit**

```bash
git add web-lit/src/pages/
git commit -m "feat(web-lit): create 13 placeholder pages"
```

---

## Phase 0.5: API Client Extraction

### Task 5: Create Vite Config and API Client

**Files:**
- Create: `web-lit/.env`
- Create: `web-lit/src/lib/config.ts`
- Create: `web-lit/src/lib/case-converter.ts`
- Create: `web-lit/src/lib/api-client.ts`

- [ ] **Step 1: Create `web-lit/.env`**

```
VITE_API_PREFIX=/api/v1
VITE_PUBLIC_API_PREFIX=/api/v1
VITE_EDITION=SELF_HOSTED
```

- [ ] **Step 2: Create `web-lit/src/lib/config.ts`**

```typescript
// Vite environment variables
// Use import.meta.env.VITE_* for client-side env vars

export const API_PREFIX = import.meta.env.VITE_API_PREFIX || '/api/v1'
export const PUBLIC_API_PREFIX = import.meta.env.VITE_PUBLIC_API_PREFIX || '/api/v1'
export const IS_CE_EDITION = import.meta.env.VITE_EDITION === 'SELF_HOSTED'
```

- [ ] **Step 3: Copy `web-lit/src/lib/case-converter.ts`**

Copy from `/Users/chenguorong/code/my/tinyiothub/web/lib/case-converter.ts` — this is pure TS, no React dependencies.

- [ ] **Step 4: Create `web-lit/src/lib/api-client.ts`**

This is adapted from `/Users/chenguorong/code/my/tinyiothub/web/lib/api-client.ts` but:
- Removes React Query imports
- Removes `@/config` import (uses local `config.ts` instead)
- Removes `@/lib/query-keys` import (not needed for pure async)
- Adds `401` refresh token interceptor

```typescript
import { keysToCamelCase, keysToSnakeCase } from './case-converter'
import { API_PREFIX } from './config'
import type { ApiResponse } from '../types'

// Types
export interface KeysToCamelCase<T> {
  [K in keyof T as Uncapitalize<string & K>]: T[K] extends object
    ? KeysToCamelCase<T[K]>
    : T[K]
}

type Uncapitalize<T extends string> = T extends `${infer F}${infer R}`
  ? `${Uncapitalize<F>}${R}`
  : T

export interface PaginatedResponse<T> {
  data: T[]
  pagination: {
    page: number
    pageSize: number
    totalPages: number
    totalCount: number
  }
}

// Get auth token from sessionStorage (more secure than localStorage)
const getAuthToken = (): string | null => {
  if (typeof window === 'undefined') return null
  return sessionStorage.getItem('auth-token')
}

// Build full URL
const buildUrl = (endpoint: string): string => {
  if (endpoint.startsWith('http://') || endpoint.startsWith('https://')) {
    return endpoint
  }
  const normalizedEndpoint = endpoint.startsWith('/') ? endpoint : `/${endpoint}`
  return `${API_PREFIX}${normalizedEndpoint}`
}

// Refresh token on 401
const refreshOnUnauthorized = async (): Promise<boolean> => {
  try {
    const response = await fetch(buildUrl('auth/refresh'), {
      method: 'POST',
      credentials: 'include',
    })
    if (response.ok) {
      const data = await response.json()
      if (data.code === 0 && data.result?.accessToken) {
        sessionStorage.setItem('auth-token', data.result.accessToken)
        return true
      }
    }
  } catch {
    // Refresh failed
  }
  return false
}

// Core HTTP request
async function request<T>(
  endpoint: string,
  options: RequestInit & { params?: Record<string, any> } = {}
): Promise<T> {
  const { method = 'GET', params, ...rest } = options

  const url = new URL(buildUrl(endpoint))
  if (params) {
    Object.entries(params).forEach(([key, value]) => {
      if (value !== undefined && value !== null) {
        url.searchParams.append(key, String(value))
      }
    })
  }

  const config: RequestInit = {
    method,
    credentials: 'include',
    headers: {
      'Content-Type': 'application/json',
    },
    ...rest,
  }

  const token = getAuthToken()
  if (token) {
    config.headers = {
      ...config.headers,
      'Authorization': `Bearer ${token}`,
    }
  }

  const response = await fetch(url.toString(), config)

  if (response.status === 401) {
    // Try to refresh token
    const refreshed = await refreshOnUnauthorized()
    if (refreshed) {
      // Retry with new token
      const newToken = sessionStorage.getItem('auth-token')
      config.headers = {
        ...config.headers,
        'Authorization': `Bearer ${newToken}`,
      }
      const retryResponse = await fetch(url.toString(), config)
      if (!retryResponse.ok) {
        throw new Error(`HTTP ${retryResponse.status}`)
      }
      return retryResponse.json()
    } else {
      // Refresh failed — clear session and dispatch event
      sessionStorage.removeItem('auth-token')
      window.dispatchEvent(new CustomEvent('auth-error', {
        detail: { message: 'Session expired' }
      }))
      throw new Error('Unauthorized')
    }
  }

  if (!response.ok) {
    let errorData: any = {}
    try {
      errorData = await response.json()
    } catch { /* ignore */ }
    const error = new Error(errorData?.msg || `HTTP ${response.status}`)
    ;(error as any).data = errorData
    throw error
  }

  return response.json()
}

// API Client class
export class ApiClient {
  static async get<T>(endpoint: string, params?: Record<string, any>): Promise<ApiResponse<KeysToCamelCase<T>>> {
    const response = await request<ApiResponse<T>>(endpoint, {
      method: 'GET',
      params: params ? keysToSnakeCase(params) : undefined,
    })
    if (response.code !== 0) {
      const error = new Error(response.msg || 'Request failed')
      ;(error as any).data = response
      ;(error as any).code = response.code
      throw error
    }
    return {
      ...response,
      result: response.result ? keysToCamelCase(response.result) : response.result,
    } as ApiResponse<KeysToCamelCase<T>>
  }

  static async post<T>(endpoint: string, data?: any): Promise<ApiResponse<KeysToCamelCase<T>>> {
    const response = await request<ApiResponse<T>>(endpoint, {
      method: 'POST',
      body: data ? keysToSnakeCase(data) : undefined,
    })
    if (response.code !== 0) {
      const error = new Error(response.msg || 'Request failed')
      ;(error as any).data = response
      ;(error as any).code = response.code
      throw error
    }
    return {
      ...response,
      result: response.result ? keysToCamelCase(response.result) : response.result,
    } as ApiResponse<KeysToCamelCase<T>>
  }

  static async put<T>(endpoint: string, data?: any): Promise<ApiResponse<KeysToCamelCase<T>>> {
    const response = await request<ApiResponse<T>>(endpoint, {
      method: 'PUT',
      body: data ? keysToSnakeCase(data) : undefined,
    })
    if (response.code !== 0) {
      const error = new Error(response.msg || 'Request failed')
      ;(error as any).data = response
      ;(error as any).code = response.code
      throw error
    }
    return {
      ...response,
      result: response.result ? keysToCamelCase(response.result) : response.result,
    } as ApiResponse<KeysToCamelCase<T>>
  }

  static async delete<T>(endpoint: string): Promise<ApiResponse<KeysToCamelCase<T>>> {
    const response = await request<ApiResponse<T>>(endpoint, { method: 'DELETE' })
    if (response.code !== 0) {
      const error = new Error(response.msg || 'Request failed')
      ;(error as any).data = response
      ;(error as any).code = response.code
      throw error
    }
    return {
      ...response,
      result: response.result ? keysToCamelCase(response.result) : response.result,
    } as ApiResponse<KeysToCamelCase<T>>
  }
}

export const { get: apiGet, post: apiPost, put: apiPut, delete: apiDelete } = ApiClient
```

- [ ] **Step 5: Commit**

```bash
git add web-lit/.env web-lit/src/lib/
git commit -m "feat(web-lit): create Vite config and API client with token refresh"
```

---

### Task 6: Create Nanostores (Auth + App)

**Files:**
- Create: `web-lit/src/stores/auth-store.ts`
- Create: `web-lit/src/stores/app-store.ts`

- [ ] **Step 1: Write `web-lit/src/stores/auth-store.ts`**

```typescript
import { atom, computed } from 'nanostores'

export interface User {
  id: string
  name: string
  email?: string
  phone?: string
  avatar?: string
}

export const $token = atom<string | null>(
  typeof window !== 'undefined' ? sessionStorage.getItem('auth-token') : null
)
export const $user = atom<User | null>(null)

export const $isAuthenticated = computed([$token], (token) => !!token)

// Persist to sessionStorage
$token.subscribe((token) => {
  if (typeof window !== 'undefined') {
    if (token) {
      sessionStorage.setItem('auth-token', token)
    } else {
      sessionStorage.removeItem('auth-token')
    }
  }
})

// Actions
export function setAuth(token: string, user: User) {
  $token.set(token)
  $user.set(user)
}

export function clearAuth() {
  $token.set(null)
  $user.set(null)
}

// Listen for 401 errors from API client
if (typeof window !== 'undefined') {
  window.addEventListener('auth-error', () => {
    clearAuth()
    window.location.href = '/signin'
  })
}
```

- [ ] **Step 2: Write `web-lit/src/stores/app-store.ts`**

```typescript
import { atom } from 'nanostores'

export const $sidebarCollapsed = atom<boolean>(false)
export const $theme = atom<'dark' | 'light'>('dark')

export function toggleSidebar() {
  $sidebarCollapsed.set(!$sidebarCollapsed.get())
}

export function setTheme(theme: 'dark' | 'light') {
  $theme.set(theme)
}
```

- [ ] **Step 3: Commit**

```bash
git add web-lit/src/stores/
git commit -m "feat(web-lit): create nanostores for auth and app state"
```

---

### Task 7: Extract Services from `web/service/`

**Files:**
- Create: `web-lit/src/services/auth.ts`
- Create: `web-lit/src/services/devices.ts`
- Create: `web-lit/src/services/alarms.ts`
- Create: `web-lit/src/services/dashboard.ts`
- Create: `web-lit/src/services/events.ts`
- Create: `web-lit/src/services/templates.ts`
- Create: `web-lit/src/services/tenant.ts`
- Create: `web-lit/src/services/users.ts`

**Pattern:** For each service file:
1. Copy from `web/service/*.ts`
2. Remove all `useQuery`, `useMutation`, `useQueryClient` imports and hook exports
3. Keep only the raw API function calls (`apiGet`, `apiPost`, etc.)
4. Export as pure async functions

**Example transformation** (from `web/service/auth.ts`):

```typescript
// BEFORE (React Query):
export const useProfile = () => {
  return useQuery({
    queryKey: ['auth', 'profile'],
    queryFn: authApi.getProfile,
  })
}

// AFTER (pure async):
export const authApi = {
  login: (data: LoginRequest) => apiPost<LoginResponse>('auth/login', data),
  logout: () => apiPost<boolean>('auth/logout'),
  getProfile: () => apiGet<UserProfile>('auth/session/profile'),
  refreshToken: () => apiPost<{ accessToken: string }>('auth/refresh'),
}
```

- [ ] **Step 1: Extract `auth.ts`**

Read `web/service/auth.ts` and `web/types/user.ts` for types.

Keep `LoginRequest`, `LoginResponse`, `UserInfo`, `UserProfile`, `ChangePasswordRequest` interfaces.
Keep `authApi` object with pure async methods.
Remove all React Query hooks (`useProfile`, `useLogin`, `useLogout`, `useUpdateProfile`, `useChangePassword`, `useRefreshToken`).

- [ ] **Step 2: Extract `devices.ts`**

Read `web/service/devices.ts` and `web/types/device.ts`.

Keep `Device`, `DeviceListParams`, device CRUD methods in `devicesApi`.
Remove `useDevices`, `useDevice`, `useCreateDevice`, etc.

- [ ] **Step 3: Extract `alarms.ts`**

Read `web/service/alarms.ts` and `web/types/alarm.ts`.

Keep alarm query, acknowledge, resolve methods.
Remove React Query hooks.

- [ ] **Step 4: Extract remaining services**

For each of: `dashboard.ts`, `events.ts`, `templates.ts`, `tenant.ts`, `users.ts`:
- Copy from `web/service/*.ts`
- Remove React Query wrappers
- Keep pure async API methods

- [ ] **Step 5: Commit**

```bash
git add web-lit/src/services/
git commit -m "feat(web-lit): extract pure async services from web/service/"
```

---

### Task 8: Copy Types and i18n

**Files:**
- Create: `web-lit/src/types/` (copy from `web/types/`)
- Create: `web-lit/src/i18n/` (copy from `web/i18n/`)

- [ ] **Step 1: Copy type definitions**

```bash
cp -r web/types/*.ts web-lit/src/types/
cp web/types/**/*.ts web-lit/src/types/ 2>/dev/null || true
```

Verify: `ls web-lit/src/types/` should have: `alarm.ts`, `dashboard.ts`, `device.ts`, `feature.ts`, `index.ts`, `system.ts`, `tag.ts`, `template.ts`, `user.ts`

Also copy `web/lib/case-converter.ts` if not already done in Task 5.

- [ ] **Step 2: Verify no React types in types**

Grep: `grep -r "React" web-lit/src/types/`
Expected: No matches. If React types found, remove them.

- [ ] **Step 3: Copy i18n resources**

```bash
cp -r web/i18n/ web-lit/src/i18n/
```

- [ ] **Step 4: Commit**

```bash
git add web-lit/src/types/ web-lit/src/i18n/
git commit -m "feat(web-lit): copy types and i18n from web/"
```

---

## Verification

After completing all tasks:

- [ ] `cd web-lit && pnpm build` should succeed
- [ ] `cd web-lit && pnpm dev` should start on port 5173
- [ ] `ls web-lit/src/` should have: `components/`, `pages/`, `services/`, `lib/`, `stores/`, `router/`, `types/`, `i18n/`, `styles/`, `app.ts`, `main.ts`
- [ ] No `process.env` usage in `src/lib/` — should use `import.meta.env`
- [ ] No React Query imports in `src/services/`

---

## Dependencies

This plan depends on:
- `/Users/chenguorong/code/github/openclaw/ui/src/styles/` (CSS source)
- `/Users/chenguorong/code/my/tinyiothub/web/service/*.ts` (service extraction)
- `/Users/chenguorong/code/my/tinyiothub/web/types/*.ts` (type copying)
- `/Users/chenguorong/code/my/tinyiothub/web/i18n/` (i18n copying)
