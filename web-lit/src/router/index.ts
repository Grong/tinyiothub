import { Router, type RouteConfig } from '@lit-labs/router'
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

// Helper to create a route config with render callback
const createRoute = (path: string, component: string): RouteConfig => ({
  path,
  render: () => {
    const el = document.createElement(component)
    return el
  },
})

export const routes: RouteConfig[] = [
  createRoute('/', 'home-page'),
  createRoute('/signin', 'signin-page'),
  createRoute('/tenant/register', 'register-page'),
  createRoute('/dashboard', 'dashboard-page'),
  createRoute('/devices', 'devices-page'),
  createRoute('/device-detail/:id', 'device-detail-page'),
  createRoute('/alarms', 'alarms-page'),
  createRoute('/monitoring', 'monitoring-page'),
  createRoute('/settings', 'settings-page'),
  createRoute('/tags', 'tags-page'),
  createRoute('/templates', 'templates-page'),
  createRoute('/marketplace', 'marketplace-page'),
  createRoute('/installed-marketplace', 'installed-marketplace-page'),
]

// Router instance - initialized when app provides container
let _router: Router | null = null

export function initRouter(container: HTMLElement & { addController?: any; removeController?: any }) {
  _router = new Router(container as any, routes)
  return _router
}

export function destroyRouter() {
  _router = null
}

// Helper to navigate - uses router's internal navigation when available
export function navigate(path: string) {
  if (_router) {
    _router.goto(path)
  } else {
    history.pushState({}, '', path)
  }
}
