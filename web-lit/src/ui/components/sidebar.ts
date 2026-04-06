import { html, nothing } from 'lit'
import type { AppViewState } from '../app-view-state'
import type { Route } from '../types'
import { icon, type IconName } from '../icons'
import { navigate } from '../../lib/navigate'

interface NavItem {
  route: Route
  label: string
  icon: IconName
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
        <span class="sidebar-logo"> </span>
        ${state.navCollapsed ? nothing : html`<span class="sidebar-title">TinyIoTHub</span>`}
      </div>
      <div class="sidebar-nav">
        ${NAV_ITEMS.map(item => html`
          <button
            class="nav-item ${state.currentRoute === item.route ? 'active' : ''}"
            @click=${() => handleNav(item.route)}
            title=${item.label}
          >
            ${icon(item.icon)}
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
