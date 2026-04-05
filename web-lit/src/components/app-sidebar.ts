/**
 * AppSidebar - Unified sidebar navigation
 *
 * Layout/nav styles come from global layout.css (.sidebar, .nav-item, etc.)
 * Only component-specific rendering logic lives here.
 */

import { LitElement, html } from 'lit'
import { customElement } from 'lit/decorators.js'
import { $currentRoute, $navCollapsed } from '../stores/app-state'
import { navigate } from '../lib/navigate'
import './logo-icon'
import './workspace-picker'

interface NavItem {
  id: string
  icon: string
  label: string
  route: string
}

interface NavSection {
  label?: string
  items: NavItem[]
}

@customElement('app-sidebar')
export class AppSidebar extends LitElement {
  createRenderRoot() {
    return this
  }

  private navSections: NavSection[] = [
    {
      items: [
        { id: 'home', icon: 'home', label: '首页', route: 'home' },
        { id: 'dashboard', icon: 'dashboard', label: '仪表盘', route: 'dashboard' },
      ],
    },
    {
      label: '设备管理',
      items: [
        { id: 'devices', icon: 'devices', label: '设备列表', route: 'devices' },
        { id: 'tags', icon: 'tags', label: '标签管理', route: 'tags' },
        { id: 'templates', icon: 'templates', label: '设备模板', route: 'templates' },
      ],
    },
    {
      label: '运维管理',
      items: [
        { id: 'alarms', icon: 'alarms', label: '告警管理', route: 'alarms' },
        { id: 'monitoring', icon: 'monitoring', label: '系统监控', route: 'monitoring' },
      ],
    },
    {
      label: '应用中心',
      items: [
        { id: 'marketplace', icon: 'marketplace', label: '应用市场', route: 'marketplace' },
        { id: 'installed', icon: 'installed', label: '已安装', route: 'installed-marketplace' },
      ],
    },
    {
      items: [
        { id: 'settings', icon: 'settings', label: '系统设置', route: 'settings' },
      ],
    },
  ]

  private _unsubNavCollapsed: (() => void) | null = null
  private _unsubCurrentRoute: (() => void) | null = null

  connectedCallback() {
    super.connectedCallback()
    this._unsubNavCollapsed = $navCollapsed.subscribe(() => this.requestUpdate())
    this._unsubCurrentRoute = $currentRoute.subscribe(() => this.requestUpdate())
  }

  disconnectedCallback() {
    super.disconnectedCallback()
    this._unsubNavCollapsed?.()
    this._unsubCurrentRoute?.()
    this._unsubNavCollapsed = null
    this._unsubCurrentRoute = null
  }

  toggleCollapse() {
    $navCollapsed.set(!$navCollapsed.get())
  }

  isActive(route: string): boolean {
    return $currentRoute.get() === route
  }

  handleNavClick(route: string, e: Event) {
    e.preventDefault()
    navigate(route)
  }

  renderIcon(name: string) {
    const icons: Record<string, ReturnType<typeof html>> = {
      'home': html`<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.5"><path d="M2.25 12l8.954-8.955c.44-.439 1.152-.439 1.591 0L21.75 12M4.5 9.75v10.125c0 .621.504 1.125 1.125 1.125H9.75v-4.875c0-.621.504-1.125 1.125-1.125h2.25c.621 0 1.125.504 1.125 1.125V21h4.125c.621 0 1.125-.504 1.125-1.125V9.75M8.25 21h8.25"/></svg>`,
      'dashboard': html`<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.5"><rect x="3" y="3" width="7" height="7" rx="1"/><rect x="14" y="3" width="7" height="7" rx="1"/><rect x="3" y="14" width="7" height="7" rx="1"/><rect x="14" y="14" width="7" height="7" rx="1"/></svg>`,
      'devices': html`<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.5"><path d="M8.25 3v1.5M4.5 8.25H3m18 0h-1.5M4.5 12H3m18 0h-1.5m-15 3.75H3m18 0h-1.5M8.25 19.5V21M12 3v1.5m0 15V21m3.75-18v1.5m0 15V21m-9-1.5h10.5a2.25 2.25 0 002.25-2.25V6.75a2.25 2.25 0 00-2.25-2.25H6.75A2.25 2.25 0 004.5 6.75v10.5a2.25 2.25 0 002.25 2.25zm.75-12h9v9h-9v-9z"/></svg>`,
      'alarms': html`<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.5"><path d="M14.857 17.082a24.06 24.06 0 01-8.835-2.084L3 21l1.035-3.194a24.06 24.06 0 018.835-2.084L15 15M6 6h12M6 10h12M6 14h8"/></svg>`,
      'monitoring': html`<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.5"><path d="M3 13.125C3 12.504 3.504 12 4.125 12h2.25c.621 0 1.125.504 1.125 1.125v6.75C7.5 20.496 6.996 21 6.375 21h-2.25A1.125 1.125 0 013 19.875v-6.75zM9.75 8.625c0-.621.504-1.125 1.125-1.125h2.25c.621 0 1.125.504 1.125 1.125v11.25c0 .621-.504 1.125-1.125 1.125h-2.25a1.125 1.125 0 01-1.125-1.125V8.625zM16.5 4.125c0-.621.504-1.125 1.125-1.125h2.25C20.496 3 21 3.504 21 4.125v15.75c0 .621-.504 1.125-1.125 1.125h-2.25a1.125 1.125 0 01-1.125-1.125V4.125z"/></svg>`,
      'settings': html`<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.5"><path d="M9.594 3.94c.09-.542.56-.94 1.11-.94h2.593c.55 0 1.02.398 1.11.94l.213 1.281c.063.374.313.686.645.87.074.04.147.083.22.127.324.196.72.257 1.075.124l1.217-.456a1.125 1.125 0 011.37.49l1.296 2.247a1.125 1.125 0 01-.26 1.431l-1.003.827c-.293.24-.438.613-.431.992a6.759 6.759 0 010 .255c-.007.378.138.75.43.99l1.005.828c.424.35.534.954.26 1.43l-1.298 2.247a1.125 1.125 0 01-1.369.491l-1.217-.456c-.355-.133-.75-.072-1.076.124a6.57 6.57 0 01-.22.128c-.331.183-.581.495-.644.869l-.213 1.28c-.09.543-.56.941-1.11.941h-2.594c-.55 0-1.02-.398-1.11-.94l-.213-1.281c-.062-.374-.312-.686-.644-.87a6.52 6.52 0 01-.22-.127c-.325-.196-.72-.257-1.076-.124l-1.217.456a1.125 1.125 0 01-1.369-.49l-1.297-2.247a1.125 1.125 0 01.26-1.431l1.004-.827c.292-.24.437-.613.43-.992a6.932 6.932 0 010-.255c.007-.378-.138-.75-.43-.99l-1.004-.828a1.125 1.125 0 01-.26-1.43l1.297-2.247a1.125 1.125 0 011.37-.491l1.216.456c.356.133.751.072 1.076-.124.072-.044.146-.087.22-.128.332-.183.582-.495.644-.869l.214-1.281z"/><path d="M15 12a3 3 0 11-6 0 3 3 0 016 0z"/></svg>`,
      'tags': html`<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.5"><path d="M9.568 3H5.25A2.25 2.25 0 003 5.25v4.318c0 .597.237 1.17.659 1.591l9.581 9.581c.699.699 1.78.872 2.607.33a18.095 18.095 0 005.223-5.223c.542-.827.369-1.908-.33-2.607L11.16 3.66A2.25 2.25 0 009.568 3z"/><path d="M6 6h.008v.008H6V6z"/></svg>`,
      'templates': html`<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.5"><path d="M20.25 7.5l-.625 10.632a2.25 2.25 0 01-2.247 2.118H6.622a2.25 2.25 0 01-2.247-2.118L3.75 7.5M10 11.25h4M3.375 7.5h17.25c.621 0 1.125-.504 1.125-1.125v-1.5c0-.621-.504-1.125-1.125-1.125H3.375c-.621 0-1.125.504-1.125 1.125v1.5c0 .621.504 1.125 1.125 1.125z"/></svg>`,
      'marketplace': html`<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.5"><path d="M12 21a9.004 9.004 0 008.716-6.747M12 21a9.004 9.004 0 01-8.716-6.747M12 21c2.485 0 4.5-4.03 4.5-9S14.485 3 12 3m0 18c-2.485 0-4.5-4.03-4.5-9S9.515 3 12 3m0 0a8.997 8.997 0 017.843 4.582M12 3a8.997 8.997 0 00-7.843 4.582m15.686 0A11.953 11.953 0 0112 10.5c-2.998 0-5.74-1.1-7.843-2.918m15.686 0A8.959 8.959 0 0121 12c0 .778-.099 1.533-.284 2.253m0 0A17.919 17.919 0 0112 16.5c-3.162 0-6.133-.815-8.716-2.247m0 0A9.015 9.015 0 013 12c0-1.605.42-3.113 1.157-4.418"/></svg>`,
      'installed': html`<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.5"><path d="M5.625 3.75a2.625 2.625 0 100 5.25h12.75a2.625 2.625 0 002.625-2.625V3.75H5.625zM3.75 12v6.75c0 .621.504 1.125 1.125 1.125h13.5c.621 0 1.125-.504 1.125-1.125V12M3 15.75c0-.621.504-1.125 1.125-1.125h13.5c.621 0 1.125.504 1.125 1.125v2.625c0 .621-.504 1.125-1.125 1.125H4.125A1.125 1.125 0 013 18.375v-2.625z"/></svg>`,
      'collapse': html`<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><path d="M3.75 6.75h16.5M3.75 12h16.5m-16.5 5.25H12"/></svg>`,
      'expand': html`<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><path d="M3.75 6.75h16.5M3.75 12h16.5m-16.5 5.25H12"/></svg>`,
    }
    return icons[name] || html``
  }

  render() {
    const collapsed = $navCollapsed.get()

    return html`
      <div class="sidebar ${collapsed ? 'sidebar--collapsed' : ''}">
        <!-- Header with logo and collapse button -->
        <div class="sidebar-shell__header">
          <div class="sidebar-brand">
            <logo-icon size="32px"></logo-icon>
            ${!collapsed ? html`
              <div class="sidebar-brand__copy">
                <span class="sidebar-brand__title">TinyIoTHub</span>
              </div>
            ` : ''}
          </div>
          <button class="nav-collapse-toggle" @click=${this.toggleCollapse} title="${collapsed ? '展开' : '收起'}">
            <span class="nav-collapse-toggle__icon">
              ${this.renderIcon('collapse')}
            </span>
          </button>
        </div>

        <!-- Workspace picker -->
        ${!collapsed ? html`<workspace-picker></workspace-picker>` : ''}

        <!-- Navigation -->
        <nav class="sidebar-nav">
          ${this.navSections.map((section) => html`
            <div class="nav-section">
              ${section.label ? html`
                <div class="nav-section__label">
                  <span class="nav-section__label-text">${section.label}</span>
                </div>
              ` : ''}
              <div class="nav-section__items">
                ${section.items.map((item) => html`
                  <a
                    class="nav-item ${this.isActive(item.route) ? 'active' : ''}"
                    @click=${(e: Event) => this.handleNavClick(item.route, e)}
                  >
                    <span class="nav-item__icon">
                      ${this.renderIcon(item.icon)}
                    </span>
                    ${!collapsed ? html`<span class="nav-item__text">${item.label}</span>` : ''}
                  </a>
                `)}
              </div>
            </div>
          `)}
        </nav>

        <!-- Footer -->
        <div class="sidebar-shell__footer">
          <div class="sidebar-version">
            ${!collapsed ? html`<span class="sidebar-version__text"></span>` : ''}
            <div class="sidebar-version__dot"></div>
          </div>
        </div>
      </div>
    `
  }
}

declare global {
  interface HTMLElementTagNameMap {
    'app-sidebar': AppSidebar
  }
}
