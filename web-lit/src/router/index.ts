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
