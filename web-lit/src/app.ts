import { LitElement, html, css } from 'lit'
import { customElement, state } from 'lit/decorators.js'
import { $isAuthenticated, $user, clearAuth } from './stores/auth-store'
import { navigate } from './lib/navigate'
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

// Pages that don't need authentication
const PUBLIC_ROUTES = ['home', 'signin', 'tenant/register']

@customElement('tinyiothub-app')
export class App extends LitElement {
  createRenderRoot() {
    return this
  }

  @state() currentRoute = 'home'
  @state() navCollapsed = false
  @state() isAuthenticated = false
  @state() user: { name?: string; email?: string } | null = null
  @state() showUserMenu = false
  @state() showNotifications = false
  @state() searchQuery = ''
  @state() alarmCount = 0

  private unsubAuth?: () => void
  private unsubUser?: () => void

  static styles = css`
    :host {
      display: block;
      min-height: 100vh;
    }

    /* Override shell to work with our routing */
    .app-shell {
      display: grid;
      grid-template-columns: var(--shell-nav-width) minmax(0, 1fr);
      grid-template-rows: var(--shell-topbar-height) 1fr;
      grid-template-areas:
        "nav topbar"
        "nav content";
      height: 100vh;
      overflow: hidden;
    }

    .app-shell--collapsed {
      grid-template-columns: var(--shell-nav-rail-width) minmax(0, 1fr);
    }

    .app-shell--public {
      grid-template-columns: 0 minmax(0, 1fr);
      grid-template-rows: var(--shell-topbar-height) 1fr;
      grid-template-areas:
        "topbar topbar"
        "content content";
    }

    .app-shell--public .sidebar,
    .app-shell--public .shell-nav {
      display: none;
    }

    /* Sidebar nav area */
    .shell-nav {
      grid-area: nav;
    }

    /* Main content area */
    .main-content {
      grid-area: content;
      display: flex;
      flex-direction: column;
      min-height: 0;
      overflow: hidden;
    }

    /* Topbar */
    .topbar {
      grid-area: topbar;
    }

    /* Content scroll area */
    .content-scroll {
      flex: 1;
      overflow-y: auto;
      overflow-x: hidden;
      padding: 16px 20px 32px;
    }

    /* User menu dropdown */
    .user-menu-dropdown {
      position: absolute;
      top: 100%;
      right: 0;
      margin-top: 8px;
      min-width: 200px;
      background: var(--card);
      border: 1px solid var(--border);
      border-radius: var(--radius-lg);
      box-shadow: 0 8px 24px rgba(0,0,0,0.15);
      z-index: 100;
      overflow: hidden;
    }

    .user-menu-item {
      display: flex;
      align-items: center;
      gap: 10px;
      padding: 10px 14px;
      color: var(--text);
      font-size: 14px;
      cursor: pointer;
      transition: background var(--duration-fast) ease;
    }

    .user-menu-item:hover {
      background: var(--bg-hover);
    }

    .user-menu-item.danger {
      color: var(--danger);
    }

    .user-menu-divider {
      height: 1px;
      background: var(--border);
      margin: 4px 0;
    }

    /* Footer */
    .footer {
      grid-area: footer;
      display: flex;
      align-items: center;
      justify-content: space-between;
      padding: 12px 20px;
      border-top: 1px solid var(--border);
      background: var(--bg-secondary);
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

    /* Search in topbar */
    .topbar-search {
      display: inline-flex;
      align-items: center;
      gap: 8px;
      padding: 6px 12px;
      border: 1px solid var(--border);
      border-radius: var(--radius-md);
      background: var(--bg);
      color: var(--muted);
      font-size: 13px;
      cursor: pointer;
      min-width: 180px;
      transition: border-color var(--duration-fast) ease, background var(--duration-fast) ease;
    }

    .topbar-search:hover {
      border-color: var(--border-strong);
      background: var(--bg-hover);
    }

    .topbar-search input {
      border: none;
      background: transparent;
      color: var(--text);
      font-size: 13px;
      outline: none;
      width: 100%;
    }

    .topbar-search input::placeholder {
      color: var(--muted);
    }

    /* User avatar */
    .user-avatar {
      width: 32px;
      height: 32px;
      border-radius: var(--radius-full);
      background: var(--accent);
      color: var(--accent-foreground);
      display: flex;
      align-items: center;
      justify-content: center;
      font-size: 13px;
      font-weight: 600;
      cursor: pointer;
      transition: opacity var(--duration-fast) ease;
    }

    .user-avatar:hover {
      opacity: 0.85;
    }

    /* Topbar icon buttons */
    .topbar-btn {
      width: 36px;
      height: 36px;
      display: flex;
      align-items: center;
      justify-content: center;
      border: 1px solid var(--border);
      border-radius: var(--radius-md);
      background: transparent;
      color: var(--muted);
      cursor: pointer;
      position: relative;
      transition: all var(--duration-fast) ease;
    }

    .topbar-btn:hover {
      background: var(--bg-hover);
      border-color: var(--border-strong);
      color: var(--text);
    }

    .topbar-btn.primary {
      background: var(--accent);
      border-color: var(--accent);
      color: var(--accent-foreground);
    }

    .topbar-btn.primary:hover {
      background: var(--accent-hover);
      border-color: var(--accent-hover);
    }

    .topbar-btn svg {
      width: 18px;
      height: 18px;
    }

    .topbar-btn .badge {
      position: absolute;
      top: -4px;
      right: -4px;
      min-width: 18px;
      height: 18px;
      padding: 0 5px;
      background: var(--danger);
      color: white;
      font-size: 10px;
      font-weight: 700;
      border-radius: var(--radius-full);
      display: flex;
      align-items: center;
      justify-content: center;
    }

    /* Notification dropdown */
    .notification-dropdown {
      position: absolute;
      top: 100%;
      right: 0;
      margin-top: 8px;
      width: 320px;
      max-height: 400px;
      background: var(--card);
      border: 1px solid var(--border);
      border-radius: var(--radius-lg);
      box-shadow: 0 8px 24px rgba(0,0,0,0.15);
      z-index: 100;
      overflow: hidden;
    }

    .notification-header {
      display: flex;
      align-items: center;
      justify-content: space-between;
      padding: 12px 16px;
      border-bottom: 1px solid var(--border);
    }

    .notification-title {
      font-size: 14px;
      font-weight: 600;
      color: var(--text);
    }

    .notification-item {
      display: flex;
      gap: 12px;
      padding: 12px 16px;
      border-bottom: 1px solid var(--border);
      cursor: pointer;
      transition: background var(--duration-fast) ease;
    }

    .notification-item:hover {
      background: var(--bg-hover);
    }

    .notification-item:last-child {
      border-bottom: none;
    }

    .notification-icon {
      width: 32px;
      height: 32px;
      border-radius: var(--radius-full);
      display: flex;
      align-items: center;
      justify-content: center;
      flex-shrink: 0;
    }

    .notification-icon.warning {
      background: var(--warn-subtle);
      color: var(--warn);
    }

    .notification-icon.danger {
      background: var(--danger-subtle);
      color: var(--danger);
    }

    .notification-icon svg {
      width: 16px;
      height: 16px;
    }

    .notification-content {
      flex: 1;
      min-width: 0;
    }

    .notification-text {
      font-size: 13px;
      color: var(--text);
      margin-bottom: 2px;
    }

    .notification-time {
      font-size: 11px;
      color: var(--muted);
    }

    .notification-empty {
      padding: 32px 16px;
      text-align: center;
      color: var(--muted);
      font-size: 13px;
    }

    /* User menu header */
    .user-menu-header {
      padding: 12px 16px;
      border-bottom: 1px solid var(--border);
      background: var(--bg-secondary);
    }

    .user-menu-name {
      font-size: 14px;
      font-weight: 600;
      color: var(--text);
      margin-bottom: 2px;
    }

    .user-menu-email {
      font-size: 12px;
      color: var(--muted);
    }

    .user-menu-status {
      display: flex;
      align-items: center;
      gap: 6px;
      margin-top: 6px;
      font-size: 11px;
      color: var(--ok);
    }

    .user-menu-status::before {
      content: '';
      width: 6px;
      height: 6px;
      border-radius: var(--radius-full);
      background: var(--ok);
    }

    /* Back to home link */
    .back-to-home {
      display: flex;
      align-items: center;
      gap: 6px;
      color: var(--muted);
      font-size: 13px;
      text-decoration: none;
      transition: color var(--duration-fast) ease;
    }

    .back-to-home:hover {
      color: var(--accent);
    }

    .back-to-home svg {
      width: 14px;
      height: 14px;
    }

    /* Responsive */
    @media (max-width: 1100px) {
      .app-shell {
        grid-template-columns: 1fr;
        grid-template-rows: auto auto 1fr;
        grid-template-areas:
          "topbar"
          "nav"
          "content";
      }

      .app-shell--collapsed .sidebar {
        width: var(--shell-nav-rail-width);
        min-width: var(--shell-nav-rail-width);
      }
    }
  `

  connectedCallback() {
    super.connectedCallback()
    this.setupRouter()
    this.subscribeToAuth()
    // Close menus when clicking outside
    document.addEventListener('click', this.handleDocumentClick.bind(this))
  }

  disconnectedCallback() {
    super.disconnectedCallback()
    window.removeEventListener('popstate', this.handleRoute.bind(this))
    this.unsubAuth?.()
    this.unsubUser?.()
  }

  private subscribeToAuth() {
    // Subscribe to auth state changes
    this.unsubAuth = $isAuthenticated.subscribe((value) => {
      this.isAuthenticated = value
    })
    this.unsubUser = $user.subscribe((value) => {
      this.user = value
    })
    // Initial values
    this.isAuthenticated = $isAuthenticated.get()
    this.user = $user.get()
  }

  setupRouter() {
    window.addEventListener('popstate', this.handleRoute.bind(this))
    this.handleRoute()
  }

  handleRoute() {
    const path = window.location.pathname.slice(1) || ''
    this.currentRoute = path === '' ? 'home' : path
    // Check auth for protected routes
    if (!PUBLIC_ROUTES.includes(this.currentRoute) && !$isAuthenticated.get()) {
      navigate('signin')
    }
  }

  navigate(route: string) {
    window.history.pushState({}, '', `/${route}`)
    this.handleRoute()
  }

  toggleNav() {
    this.navCollapsed = !this.navCollapsed
  }

  handleLogout() {
    clearAuth()
    navigate('signin')
  }

  toggleUserMenu() {
    this.showUserMenu = !this.showUserMenu
  }

  handleDocumentClick(e: Event) {
    // Close dropdowns when clicking outside
    const target = e.target as HTMLElement
    if (!target.closest('.topbar-btn') && !target.closest('.user-avatar') && !target.closest('.user-menu-dropdown') && !target.closest('.notification-dropdown')) {
      this.showUserMenu = false
      this.showNotifications = false
    }
  }

  handleSearchKeydown(e: KeyboardEvent) {
    if (e.key === 'Enter' && this.searchQuery.trim()) {
      // TODO: Implement global search
      console.log('Search:', this.searchQuery)
    }
  }

  getPageTitle(): string {
    const titles: Record<string, string> = {
      'home': '首页',
      'dashboard': '仪表盘',
      'devices': '设备管理',
      'device-detail': '设备详情',
      'alarms': '告警管理',
      'monitoring': '系统监控',
      'settings': '系统设置',
      'tags': '标签管理',
      'templates': '设备模板',
      'marketplace': '应用市场',
      'installed-marketplace': '已安装',
    }
    return titles[this.currentRoute] || 'TinyIoTHub'
  }

  getUserInitials(): string {
    if (!this.user) return '?'
    if (this.user.name) return this.user.name.charAt(0).toUpperCase()
    if (this.user.email) return this.user.email.charAt(0).toUpperCase()
    return '?'
  }

  isActive(route: string): boolean {
    return this.currentRoute === route || window.location.pathname.slice(1) === route
  }

  renderNavItem(icon: string, text: string, route: string) {
    return html`
      <a class="nav-item ${this.isActive(route) ? 'active' : ''}"
         @click=${(e: Event) => { e.preventDefault(); this.navigate(route) }}>
        <span class="nav-item__icon">
          ${this.renderIcon(icon)}
        </span>
        <span class="nav-item__text">${text}</span>
      </a>
    `
  }

  renderIcon(name: string) {
    const icons: Record<string, ReturnType<typeof html>> = {
      'dashboard': html`<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.5"><rect x="3" y="3" width="7" height="7" rx="1"/><rect x="14" y="3" width="7" height="7" rx="1"/><rect x="3" y="14" width="7" height="7" rx="1"/><rect x="14" y="14" width="7" height="7" rx="1"/></svg>`,
      'devices': html`<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.5"><path d="M8.25 3v1.5M4.5 8.25H3m18 0h-1.5M4.5 12H3m18 0h-1.5m-15 3.75H3m18 0h-1.5M8.25 19.5V21M12 3v1.5m0 15V21m3.75-18v1.5m0 15V21m-9-1.5h10.5a2.25 2.25 0 002.25-2.25V6.75a2.25 2.25 0 00-2.25-2.25H6.75A2.25 2.25 0 004.5 6.75v10.5a2.25 2.25 0 002.25 2.25zm.75-12h9v9h-9v-9z"/></svg>`,
      'alarms': html`<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.5"><path d="M14.857 17.082a24.06 24.06 0 01-8.835-2.084L3 21l1.035-3.194a24.06 24.06 0 018.835-2.084L15 15M6 6h12M6 10h12M6 14h8"/></svg>`,
      'monitoring': html`<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.5"><path d="M3 13.125C3 12.504 3.504 12 4.125 12h2.25c.621 0 1.125.504 1.125 1.125v6.75C7.5 20.496 6.996 21 6.375 21h-2.25A1.125 1.125 0 013 19.875v-6.75zM9.75 8.625c0-.621.504-1.125 1.125-1.125h2.25c.621 0 1.125.504 1.125 1.125v11.25c0 .621-.504 1.125-1.125 1.125h-2.25a1.125 1.125 0 01-1.125-1.125V8.625zM16.5 4.125c0-.621.504-1.125 1.125-1.125h2.25C20.496 3 21 3.504 21 4.125v15.75c0 .621-.504 1.125-1.125 1.125h-2.25a1.125 1.125 0 01-1.125-1.125V4.125z"/></svg>`,
      'settings': html`<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.5"><path d="M9.594 3.94c.09-.542.56-.94 1.11-.94h2.593c.55 0 1.02.398 1.11.94l.213 1.281c.063.374.313.686.645.87.074.04.147.083.22.127.324.196.72.257 1.075.124l1.217-.456a1.125 1.125 0 011.37.49l1.296 2.247a1.125 1.125 0 01-.26 1.431l-1.003.827c-.293.24-.438.613-.431.992a6.759 6.759 0 010 .255c-.007.378.138.75.43.99l1.005.828c.424.35.534.954.26 1.43l-1.298 2.247a1.125 1.125 0 01-1.369.491l-1.217-.456c-.355-.133-.75-.072-1.076.124a6.57 6.57 0 01-.22.128c-.331.183-.581.495-.644.869l-.213 1.28c-.09.543-.56.941-1.11.941h-2.594c-.55 0-1.02-.398-1.11-.94l-.213-1.281c-.062-.374-.312-.686-.644-.87a6.52 6.52 0 01-.22-.127c-.325-.196-.72-.257-1.076-.124l-1.217.456a1.125 1.125 0 01-1.369-.49l-1.297-2.247a1.125 1.125 0 01.26-1.431l1.004-.827c.292-.24.437-.613.43-.992a6.932 6.932 0 010-.255c.007-.378-.138-.75-.43-.99l-1.004-.828a1.125 1.125 0 01-.26-1.43l1.297-2.247a1.125 1.125 0 011.37-.491l1.216.456c.356.133.751.072 1.076-.124.072-.044.146-.087.22-.128.332-.183.582-.495.644-.869l.214-1.281z"/><path d="M15 12a3 3 0 11-6 0 3 3 0 016 0z"/></svg>`,
      'tags': html`<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.5"><path d="M9.568 3H5.25A2.25 2.25 0 003 5.25v4.318c0 .597.237 1.17.659 1.591l9.581 9.581c.699.699 1.78.872 2.607.33a18.095 18.095 0 005.223-5.223c.542-.827.369-1.908-.33-2.607L11.16 3.66A2.25 2.25 0 009.568 3z"/><path d="M6 6h.008v.008H6V6z"/></svg>`,
      'templates': html`<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.5"><path d="M20.25 7.5l-.625 10.632a2.25 2.25 0 01-2.247 2.118H6.622a2.25 2.25 0 01-2.247-2.118L3.75 7.5M10 11.25h4M3.375 7.5h17.25c.621 0 1.125-.504 1.125-1.125v-1.5c0-.621-.504-1.125-1.125-1.125H3.375c-.621 0-1.125.504-1.125 1.125v1.5c0 .621.504 1.125 1.125 1.125z"/></svg>`,
      'marketplace': html`<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.5"><path d="M12 21a9.004 9.004 0 008.716-6.747M12 21a9.004 9.004 0 01-8.716-6.747M12 21c2.485 0 4.5-4.03 4.5-9S14.485 3 12 3m0 18c-2.485 0-4.5-4.03-4.5-9S9.515 3 12 3m0 0a8.997 8.997 0 017.843 4.582M12 3a8.997 8.997 0 00-7.843 4.582m15.686 0A11.953 11.953 0 0112 10.5c-2.998 0-5.74-1.1-7.843-2.918m15.686 0A8.959 8.959 0 0121 12c0 .778-.099 1.533-.284 2.253m0 0A17.919 17.919 0 0112 16.5c-3.162 0-6.133-.815-8.716-2.247m0 0A9.015 9.015 0 013 12c0-1.605.42-3.113 1.157-4.418"/></svg>`,
      'installed': html`<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.5"><path d="M5.625 3.75a2.625 2.625 0 100 5.25h12.75a2.625 2.625 0 002.625-2.625V3.75H5.625zM3.75 12v6.75c0 .621.504 1.125 1.125 1.125h13.5c.621 0 1.125-.504 1.125-1.125V12M3 15.75c0-.621.504-1.125 1.125-1.125h13.5c.621 0 1.125.504 1.125 1.125v2.625c0 .621-.504 1.125-1.125 1.125H4.125A1.125 1.125 0 013 18.375v-2.625z"/></svg>`,
      'search': html`<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.5"><path d="M21 21l-5.197-5.197m0 0A7.5 7.5 0 105.196 5.196a7.5 7.5 0 0010.607 10.607z"/></svg>`,
      'chevron-left': html`<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.5"><path d="M15.75 19.5L8.25 12l7.5-7.5"/></svg>`,
      'chevron-right': html`<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.5"><path d="M8.25 4.5l7.5 7.5-7.5 7.5"/></svg>`,
      'home': html`<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.5"><path d="M2.25 12l8.954-8.955c.44-.439 1.152-.439 1.591 0L21.75 12M4.5 9.75v10.125c0 .621.504 1.125 1.125 1.125H9.75v-4.875c0-.621.504-1.125 1.125-1.125h2.25c.621 0 1.125.504 1.125 1.125V21h4.125c.621 0 1.125-.504 1.125-1.125V9.75M8.25 21h8.25"/></svg>`,
      'user': html`<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.5"><path d="M15.75 6a3.75 3.75 0 11-7.5 0 3.75 3.75 0 017.5 0zM4.501 20.118a7.5 7.5 0 0114.998 0A17.933 17.933 0 0112 21.75c-2.676 0-5.216-.584-7.499-1.632z"/></svg>`,
      'logout': html`<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.5"><path d="M15.75 9V5.25A2.25 2.25 0 0013.5 3h-6a2.25 2.25 0 00-2.25 2.25v13.5A2.25 2.25 0 007.5 21h6a2.25 2.25 0 002.25-2.25V15M12 9l-3 3m0 0l3 3m-3-3h12.75"/></svg>`,
      'collapse': html`<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.5"><path d="M3.75 6.75h16.5M3.75 12h16.5m-16.5 5.25H12"/></svg>`,
      'expand': html`<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.5"><path d="M3.75 6.75h16.5M3.75 12h16.5m-16.5 5.25H12"/></svg>`,
      'bell': html`<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.5"><path d="M14.857 17.082a24.06 24.06 0 01-8.835-2.084L3 21l1.035-3.194a24.06 24.06 0 018.835-2.084L15 15M6 6h12M6 10h12M6 14h8"/></svg>`,
      'help': html`<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.5"><path d="M9.879 7.519c1.171-1.025 3.071-1.025 4.242 0 1.172 1.025 1.172 2.687 0 3.712-.203.179-.43.326-.67.442-.745.361-1.45.999-1.45 1.827v.75M21 12a9 9 0 11-18 0 9 9 0 0118 0zm-9 5.25h.008v.008H12v-.008z"/></svg>`,
      'sun': html`<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.5"><path d="M12 3v2.25m6.364.386l-1.591 1.591M21 12h-2.25m-.386 6.364l-1.591-1.591M12 18.75V21m-4.773-4.227l-1.591 1.591M5.25 12H3m4.227-4.773L5.636 5.636M15.75 12a3.75 3.75 0 11-7.5 0 3.75 3.75 0 017.5 0z"/></svg>`,
      'moon': html`<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.5"><path d="M21.752 15.002A9.718 9.718 0 0118 15.75c-5.385 0-9.75-4.365-9.75-9.75 0-1.33.266-2.597.748-3.752A9.753 9.753 0 003 11.25C3 16.635 7.365 21 12.75 21a9.753 9.753 0 009.002-5.998z"/></svg>`,
      'arrow-left': html`<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.5"><path d="M10.5 19.5L3 12m0 0l7.5-7.5M3 12h18"/></svg>`,
    }
    return icons[name] || html``
  }

  render() {
    const isPublic = PUBLIC_ROUTES.includes(this.currentRoute)
    const shellClass = `app-shell ${this.navCollapsed ? 'app-shell--collapsed' : ''} ${isPublic ? 'app-shell--public' : ''}`

    return html`
      <div class="${shellClass}">
        <!-- Sidebar Navigation -->
        ${!isPublic ? html`
          <nav class="shell-nav">
            <div class="sidebar ${this.navCollapsed ? 'sidebar--collapsed' : ''}">
              <div class="sidebar-shell">
                <div class="sidebar-shell__header">
                  <div class="sidebar-brand">
                    <img class="sidebar-brand__logo" src="data:image/svg+xml,<svg xmlns='http://www.w3.org/2000/svg' viewBox='0 0 24 24' fill='none' stroke='%2300d4aa' stroke-width='2'><path d='M8.25 3v1.5M4.5 8.25H3m18 0h-1.5M4.5 12H3m18 0h-1.5m-15 3.75H3m18 0h-1.5M8.25 19.5V21M12 3v1.5m0 15V21m3.75-18v1.5m0 15V21m-9-1.5h10.5a2.25 2.25 0 002.25-2.25V6.75a2.25 2.25 0 00-2.25-2.25H6.75A2.25 2.25 0 004.5 6.75v10.5a2.25 2.25 0 002.25 2.25zm.75-12h9v9h-9v-9z'/></svg>" alt="TinyIoTHub" />
                    ${!this.navCollapsed ? html`
                      <div class="sidebar-brand__copy">
                        <span class="sidebar-brand__title">TinyIoTHub</span>
                      </div>
                    ` : ''}
                  </div>
                  <button class="nav-collapse-toggle" @click=${this.toggleNav} title="${this.navCollapsed ? '展开导航' : '收起导航'}">
                    <span class="nav-collapse-toggle__icon">
                      ${this.navCollapsed ? this.renderIcon('expand') : this.renderIcon('collapse')}
                    </span>
                  </button>
                </div>

                <div class="sidebar-shell__body">
                  <nav class="sidebar-nav">
                    <div class="nav-section">
                      <div class="nav-section__items">
                        ${this.renderNavItem('home', '首页', 'home')}
                        ${this.renderNavItem('dashboard', '仪表盘', 'dashboard')}
                      </div>
                    </div>

                    <div class="nav-section">
                      <div class="nav-section__label">${this.navCollapsed ? '' : '设备管理'}</div>
                      <div class="nav-section__items">
                        ${this.renderNavItem('devices', '设备列表', 'devices')}
                        ${this.renderNavItem('tags', '标签管理', 'tags')}
                        ${this.renderNavItem('templates', '设备模板', 'templates')}
                      </div>
                    </div>

                    <div class="nav-section">
                      <div class="nav-section__label">${this.navCollapsed ? '' : '运维管理'}</div>
                      <div class="nav-section__items">
                        ${this.renderNavItem('alarms', '告警管理', 'alarms')}
                        ${this.renderNavItem('monitoring', '系统监控', 'monitoring')}
                      </div>
                    </div>

                    <div class="nav-section">
                      <div class="nav-section__label">${this.navCollapsed ? '' : '应用中心'}</div>
                      <div class="nav-section__items">
                        ${this.renderNavItem('marketplace', '应用市场', 'marketplace')}
                        ${this.renderNavItem('installed', '已安装', 'installed-marketplace')}
                      </div>
                    </div>

                    <div class="nav-section">
                      <div class="nav-section__items">
                        ${this.renderNavItem('settings', '系统设置', 'settings')}
                      </div>
                    </div>
                  </nav>
                </div>

                <div class="sidebar-shell__footer">
                  <div class="sidebar-utility-group">
                    <div class="sidebar-version">
                      <span class="sidebar-version__label">${this.navCollapsed ? '' : 'v1.0'}</span>
                      <div class="sidebar-version__dot"></div>
                    </div>
                  </div>
                </div>
              </div>
            </div>
          </nav>
        ` : ''}

        <!-- Topbar -->
        <header class="topbar">
          <div class="topnav-shell">
            ${isPublic ? html`
              <!-- Public pages: logo + nav buttons -->
              <div class="sidebar-brand">
                <img class="sidebar-brand__logo" src="data:image/svg+xml,<svg xmlns='http://www.w3.org/2000/svg' viewBox='0 0 24 24' fill='none' stroke='%2300d4aa' stroke-width='2'><path d='M8.25 3v1.5M4.5 8.25H3m18 0h-1.5M4.5 12H3m18 0h-1.5m-15 3.75H3m18 0h-1.5M8.25 19.5V21M12 3v1.5m0 15V21m3.75-18v1.5m0 15V21m-9-1.5h10.5a2.25 2.25 0 002.25-2.25V6.75a2.25 2.25 0 00-2.25-2.25H6.75A2.25 2.25 0 004.5 6.75v10.5a2.25 2.25 0 002.25 2.25zm.75-12h9v9h-9v-9z'/></svg>" alt="TinyIoTHub" style="width:28px;height:28px;" />
                <span class="sidebar-brand__title">TinyIoTHub</span>
              </div>
              <nav style="display:flex;gap:8px;margin-left:auto;">
                <button class="topbar-btn" style="width:auto;padding:0 16px;gap:6px;font-size:13px;" @click=${() => this.navigate('signin')}>
                  登录
                </button>
                <button class="topbar-btn primary" style="width:auto;padding:0 16px;gap:6px;font-size:13px;background:var(--accent);color:var(--accent-foreground);border-color:var(--accent);" @click=${() => this.navigate('tenant/register')}>
                  注册
                </button>
              </nav>
            ` : html`
              <!-- Authenticated pages: full topbar -->
              <div class="topnav-shell__content">
                <div class="dashboard-header">
                  <div class="dashboard-header__breadcrumb">
                    <span class="dashboard-header__breadcrumb-current">${this.getPageTitle()}</span>
                  </div>
                </div>
              </div>

              <div class="topnav-shell__actions">
                <div class="topbar-search">
                  ${this.renderIcon('search')}
                  <input
                    type="text"
                    placeholder="搜索设备、告警..."
                    .value=${this.searchQuery}
                    @input=${(e: InputEvent) => this.searchQuery = (e.target as HTMLInputElement).value}
                    @keydown=${this.handleSearchKeydown}
                  />
                </div>

                <!-- Help button -->
                <button class="topbar-btn" title="帮助文档">
                  ${this.renderIcon('help')}
                </button>

                <!-- Notifications -->
                <div style="position: relative;">
                  <button class="topbar-btn" title="通知" @click=${(e: Event) => { e.stopPropagation(); this.showNotifications = !this.showNotifications; this.showUserMenu = false; }}>
                    ${this.renderIcon('bell')}
                    ${this.alarmCount > 0 ? html`<span class="badge">${this.alarmCount > 99 ? '99+' : this.alarmCount}</span>` : ''}
                  </button>
                  ${this.showNotifications ? html`
                    <div class="notification-dropdown" @click=${(e: Event) => e.stopPropagation()}>
                      <div class="notification-header">
                        <span class="notification-title">通知</span>
                        ${this.alarmCount > 0 ? html`<span style="font-size:11px;color:var(--muted)">${this.alarmCount} 条未读</span>` : ''}
                      </div>
                      <div class="notification-empty">
                        暂无新通知
                      </div>
                    </div>
                  ` : ''}
                </div>

                <!-- Theme toggle -->
                <button class="topbar-btn" title="切换主题">
                  ${this.renderIcon('moon')}
                </button>

                <!-- User menu -->
                <div style="position: relative;">
                  <div class="user-avatar" @click=${(e: Event) => { e.stopPropagation(); this.toggleUserMenu(); this.showNotifications = false; }} title="${this.user?.name || this.user?.email || '用户'}">
                    ${this.getUserInitials()}
                  </div>
                  ${this.showUserMenu ? html`
                    <div class="user-menu-dropdown" @click=${(e: Event) => e.stopPropagation()}>
                      <div class="user-menu-header">
                        <div class="user-menu-name">${this.user?.name || '管理员'}</div>
                        <div class="user-menu-email">${this.user?.email || 'admin@tinyiothub.com'}</div>
                        <div class="user-menu-status">在线</div>
                      </div>
                      <div class="user-menu-item" @click=${() => { this.navigate('settings'); this.showUserMenu = false; }}>
                        ${this.renderIcon('user')}
                        <span>个人资料</span>
                      </div>
                      <div class="user-menu-item" @click=${() => { this.navigate('settings'); this.showUserMenu = false; }}>
                        ${this.renderIcon('settings')}
                        <span>设置</span>
                      </div>
                      <div class="user-menu-divider"></div>
                      <div class="user-menu-item danger" @click=${() => { this.handleLogout(); this.showUserMenu = false; }}>
                        ${this.renderIcon('logout')}
                        <span>退出登录</span>
                      </div>
                    </div>
                  ` : ''}
                </div>
              </div>
            `}
          </div>
        </header>

        <!-- Main Content -->
        <div class="main-content">
          <div class="content-scroll">
            ${this.renderPage()}
          </div>

          <!-- Footer -->
          ${!isPublic ? html`
            <footer class="footer">
              <span>© 2024 TinyIoTHub. All rights reserved.</span>
              <div class="footer-links">
                <a href="/docs" target="_blank">文档</a>
                <a href="/support" target="_blank">支持</a>
                <a href="/privacy" target="_blank">隐私政策</a>
              </div>
            </footer>
          ` : ''}
        </div>
      </div>
    `
  }

  renderPage() {
    switch (this.currentRoute) {
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
