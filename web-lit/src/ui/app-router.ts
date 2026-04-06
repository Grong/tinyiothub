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
