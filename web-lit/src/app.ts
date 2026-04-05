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

@customElement('tinyiothub-app')
export class App extends LitElement {
  createRenderRoot() {
    return this
  }

  // References to store unsubscribe functions
  private unsubs: (() => void)[] = []

  static styles = css`
    tinyiothub-app {
      display: block;
      min-height: 100vh;
    }

    /* app-header occupies the topbar grid area */
    app-header {
      grid-area: topbar;
      display: block;
    }

    /* Footer */
    .footer {
      display: flex;
      align-items: center;
      justify-content: space-between;
      padding: 12px 20px;
      box-shadow: 0 -1px 0 var(--card-highlight);      background: var(--bg-secondary);
      font-size: 12px;
      color: var(--muted);
    }

    .footer-links {
      display: flex;
      gap: 16px;
    }

    .footer-links a {
      color: var(--muted);
      text-decoration: none;
      transition: color var(--duration-fast) ease;
    }

    .footer-links a:hover {
      color: var(--text);
    }
  `

  connectedCallback() {
    super.connectedCallback()
    this.setupRouter()
    this.subscribeToStores()
  }

  disconnectedCallback() {
    super.disconnectedCallback()
    window.removeEventListener('popstate', this.handleRoute.bind(this))
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

  setupRouter() {
    window.addEventListener('popstate', this.handleRoute.bind(this))
    this.handleRoute()
  }

  handleRoute() {
    const path = window.location.pathname.slice(1) || ''
    const route = path === '' ? 'home' : path
    setCurrentRoute(route)
    // Check auth for protected routes
    if (!PUBLIC_ROUTES.includes(route) && !$isAuthenticated.get()) {
      navigate('signin')
    }
  }

  navigate(route: string) {
    setCurrentRoute(route)
    window.history.pushState({}, '', `/${route}`)
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

          <footer class="footer">
            <span>© 2024 TinyIoTHub. All rights reserved.</span>
            <div class="footer-links">
              <a href="https://docs.tinyiothub.com" target="_blank" rel="noopener">文档</a>
              <a href="/support" target="_blank">支持</a>
              <a href="/privacy" target="_blank">隐私政策</a>
            </div>
          </footer>
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
