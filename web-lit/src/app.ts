import { LitElement, html, css } from 'lit'
import { customElement } from 'lit/decorators.js'
import { $isAuthenticated } from './stores/auth-store'
import {
  $currentRoute,
  $navCollapsed,
  PUBLIC_ROUTES,
  setCurrentRoute,
} from './stores/app-state'
import { navigate } from './lib/navigate'
import './styles/base.css'
import './styles/layout.css'
import './styles/components.css'
import './components/logo-icon'
import './components/app-header'
import './components/app-sidebar'
import './pages/home-page'
import './pages/signin-page'
import './pages/register-page'
import './pages/dashboard-page'
import './pages/devices-page'
import './pages/device-detail-page'
import './pages/alarms-page'
import './pages/monitoring-page'
import './pages/settings-page'
import './pages/tags-page'
import './pages/templates-page'
import './pages/marketplace-page'
import './pages/installed-marketplace-page'
import './pages/agent-page'

@customElement('tinyiothub-app')
export class App extends LitElement {
  createRenderRoot() {
    return this
  }

  // References to store unsubscribe functions
  private unsubs: (() => void)[] = []

  
  connectedCallback() {
    super.connectedCallback()
    this.setupRouter()
    this.subscribeToStores()
  }

  disconnectedCallback() {
    super.disconnectedCallback()
    window.removeEventListener('popstate', this._onPopstate)
    this.unsubs.forEach((unsub) => unsub())
    this.unsubs = []
  }

  private subscribeToStores() {
    // Subscribe to auth state changes - re-render when auth changes
    this.unsubs.push(
      $isAuthenticated.subscribe(() => this.requestUpdate()),
      $currentRoute.subscribe(() => this.requestUpdate()),
      $navCollapsed.subscribe(() => this.requestUpdate())
    )
  }

  private _onPopstate = () => this.handleRoute()

  setupRouter() {
    window.addEventListener('popstate', this._onPopstate)
    this.handleRoute()
  }

  handleRoute() {
    const pathname = window.location.pathname
    // Only use the path portion, ignore query string
    const route = pathname === '/' ? 'home' : pathname.slice(1).split('?')[0]
    setCurrentRoute(route)
    // Check auth for protected routes
    if (!PUBLIC_ROUTES.includes(route) && !$isAuthenticated.get()) {
      navigate('signin')
    }
  }

  render() {
    const currentRoute = $currentRoute.get()
    const isPublic = PUBLIC_ROUTES.includes(currentRoute)
    const navCollapsed = $navCollapsed.get()

    // Auth pages (signin, register): full-screen, no header
    const isAuthPage = currentRoute === 'signin' || currentRoute === 'register' || currentRoute === 'tenant/register'
    if (isAuthPage) {
      return html`${this.renderPage()}`
    }

    // Public pages: topbar + content, no sidebar shell
    if (isPublic) {
      return html`
        <app-header></app-header>
        ${this.renderPage()}
      `
    }

    // Authenticated pages: shell layout with sidebar + topbar
    const shellClass = `shell ${navCollapsed ? 'shell--nav-collapsed' : ''}`

    return html`
      <div class="${shellClass}">
        <!-- Sidebar Navigation -->
        <div class="shell-nav">
          <app-sidebar></app-sidebar>
        </div>

        <!-- Main Content -->
        <div class="content">
          ${this.renderPage()}
        </div>
      </div>
    `
  }

  renderPage() {
    const currentRoute = $currentRoute.get()
    switch (currentRoute) {
      case 'home':
        return html`<home-page></home-page>`
      case 'signin':
        return html`<signin-page></signin-page>`
      case 'tenant/register':
      case 'register':
        return html`<register-page></register-page>`
      case 'dashboard':
        return html`<dashboard-page></dashboard-page>`
      case 'devices':
        return html`<devices-page></devices-page>`
      case 'device-detail':
        return html`<device-detail-page></device-detail-page>`
      case 'alarms':
        return html`<alarms-page></alarms-page>`
      case 'monitoring':
        return html`<monitoring-page></monitoring-page>`
      case 'settings':
        return html`<settings-page></settings-page>`
      case 'tags':
        return html`<tags-page></tags-page>`
      case 'templates':
        return html`<templates-page></templates-page>`
      case 'marketplace':
        return html`<marketplace-page></marketplace-page>`
      case 'installed-marketplace':
        return html`<installed-marketplace-page></installed-marketplace-page>`
      case 'agent':
        return html`<agent-page></agent-page>`
      default:
        return html`<home-page></home-page>`
    }
  }
}

declare global {
  interface HTMLElementTagNameMap {
    'tinyiothub-app': App
  }
}
