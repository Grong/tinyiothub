import { html } from 'lit'
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
