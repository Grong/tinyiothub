import { LitElement, html, css } from 'lit'
import { customElement, state } from 'lit/decorators.js'
import './styles/base.css'
import './styles/layout.css'
import './styles/components.css'
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

  @state() currentRoute = 'home'
  @state() navCollapsed = false

  static styles = css`
    :host {
      display: block;
      min-height: 100vh;
    }
    .app-shell {
      display: flex;
      min-height: 100vh;
    }
    .sidebar {
      width: 240px;
      background: var(--bg-secondary, #1a1a1a);
      flex-shrink: 0;
    }
    .main-content {
      flex: 1;
      display: flex;
      flex-direction: column;
      min-width: 0;
    }
    .topbar {
      height: 56px;
      background: var(--bg-primary, #0a0a0a);
      border-bottom: 1px solid var(--border-color, #2a2a2a);
      display: flex;
      align-items: center;
      padding: 0 24px;
      font-weight: 600;
    }
    .content {
      flex: 1;
      display: flex;
      flex-direction: column;
    }
  `

  connectedCallback() {
    super.connectedCallback()
    this.setupRouter()
  }

  disconnectedCallback() {
    super.disconnectedCallback()
    window.removeEventListener('popstate', this.handleRoute.bind(this))
  }

  setupRouter() {
    window.addEventListener('popstate', this.handleRoute.bind(this))
    this.handleRoute()
  }

  handleRoute() {
    const path = window.location.pathname.slice(1) || ''
    this.currentRoute = path === '' ? 'home' : path
  }

  navigate(route: string) {
    window.history.pushState({}, '', `/${route}`)
    this.handleRoute()
  }

  toggleNav() {
    this.navCollapsed = !this.navCollapsed
  }

  render() {
    return html`
      <div class="app-shell">
        <div class="sidebar">Sidebar</div>
        <div class="main-content">
          <header class="topbar">TinyIoTHub</header>
          <div class="content">
            ${this.currentRoute === 'home' ? html`<home-page></home-page>` : ''}
            ${this.currentRoute === 'signin' ? html`<signin-page></signin-page>` : ''}
            ${this.currentRoute === 'tenant/register' ? html`<register-page></register-page>` : ''}
            ${this.currentRoute === 'dashboard' ? html`<dashboard-page></dashboard-page>` : ''}
            ${this.currentRoute === 'devices' ? html`<devices-page></devices-page>` : ''}
            ${this.currentRoute === 'device-detail' ? html`<device-detail-page></device-detail-page>` : ''}
            ${this.currentRoute === 'alarms' ? html`<alarms-page></alarms-page>` : ''}
            ${this.currentRoute === 'monitoring' ? html`<monitoring-page></monitoring-page>` : ''}
            ${this.currentRoute === 'settings' ? html`<settings-page></settings-page>` : ''}
            ${this.currentRoute === 'tags' ? html`<tags-page></tags-page>` : ''}
            ${this.currentRoute === 'templates' ? html`<templates-page></templates-page>` : ''}
            ${this.currentRoute === 'marketplace' ? html`<marketplace-page></marketplace-page>` : ''}
            ${this.currentRoute === 'installed-marketplace' ? html`<installed-marketplace-page></installed-marketplace-page>` : ''}
          </div>
        </div>
      </div>
    `
  }
}

declare global {
  interface HTMLElementTagNameMap {
    'tinyiothub-app': App
  }
}
