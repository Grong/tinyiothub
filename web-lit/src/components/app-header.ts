/**
 * AppHeader - Unified header component
 *
 * ALL pages use this header for consistency:
 * - Logo
 * - Navigation links
 * - Auth actions (sign in/register OR user menu)
 *
 * Renders differently based on authentication state:
 * - Not authenticated: Sign In + Register buttons
 * - Authenticated: Console button + User avatar + dropdown
 */

import { LitElement, html, css } from 'lit'
import { customElement, state, property } from 'lit/decorators.js'
import { $user, $isAuthenticated } from '../stores/auth-store'
import { $searchQuery, $currentRoute } from '../stores/app-state'
import { navigate } from '../lib/navigate'
import './logo-icon'

@customElement('app-header')
export class AppHeader extends LitElement {
  @property({ type: Boolean }) backend = false
  @state() showUserMenu = false
  @state() showMobileMenu = false

  static styles = css`
    app-header {
      display: block;
    }

    /* Topbar - Glassmorphism, positioned by grid */
    .topbar {
      display: flex;
      align-items: center;
      justify-content: space-between;
      height: 64px;
      padding: 0 24px;
      background: var(--chrome);
      backdrop-filter: blur(12px);
      -webkit-backdrop-filter: blur(12px);
      box-shadow: 0 1px 8px rgba(0, 0, 0, 0.12);
    }

    /* Left section: Logo + Nav */
    .topbar-left {
      display: flex;
      align-items: center;
      gap: 32px;
    }

    .logo-link {
      display: flex;
      align-items: center;
      gap: 10px;
      text-decoration: none;
      cursor: pointer;
    }

    .logo-text {
      font-size: 20px;
      font-weight: 700;
      color: var(--text-strong);
      letter-spacing: -0.02em;
    }

    /* Navigation */
    .topbar-nav {
      display: flex;
      align-items: center;
      gap: 4px;
    }

    .nav-link {
      display: flex;
      align-items: center;
      height: 36px;
      padding: 0 14px;
      font-size: 14px;
      font-weight: 500;
      color: var(--text);
      text-decoration: none;
      border-radius: var(--radius-md);
      cursor: pointer;
      transition: all 0.15s ease;
    }

    .nav-link:hover {
      background: var(--bg-hover);
      color: var(--text-strong);
    }

    .nav-link.active {
      color: var(--accent);
      background: var(--accent-subtle);
    }

    /* Right section: Actions */
    .topbar-right {
      display: flex;
      align-items: center;
      gap: 12px;
    }

    /* Auth buttons (not logged in) */
    .auth-buttons {
      display: flex;
      align-items: center;
      gap: 8px;
    }

    .btn {
      height: 36px;
      padding: 0 18px;
      font-size: 14px;
      font-weight: 500;
      border-radius: var(--radius-md);
      cursor: pointer;
      transition: all 0.15s ease;
      text-decoration: none;
      display: inline-flex;
      align-items: center;
    }

    .btn-ghost {
      background: transparent;
      color: var(--text);
      box-shadow: var(--glass-shadow-sm);
    }

    .btn-ghost:hover {
      background: var(--bg-hover);
      color: var(--text-strong);
      box-shadow: var(--glass-shadow);
    }

    .btn-primary {
      background: var(--accent);
      color: var(--accent-foreground);
      border: none;
    }

    .btn-primary:hover {
      background: var(--accent-hover);
    }

    /* User menu (logged in) */
    .user-section {
      display: flex;
      align-items: center;
      gap: 8px;
    }

    /* Icon buttons */
    .topbar-btn {
      width: 36px;
      height: 36px;
      display: flex;
      align-items: center;
      justify-content: center;
      border-radius: var(--radius-md);
      background: transparent;
      color: var(--muted);
      cursor: pointer;
      position: relative;
      transition: all 0.15s ease;
    }

    .topbar-btn:hover {
      background: var(--bg-hover);
      color: var(--text);
      box-shadow: var(--glass-shadow-sm);
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

    /* User avatar */
    .user-avatar {
      width: 36px;
      height: 36px;
      border-radius: var(--radius-full);
      background: var(--accent);
      color: var(--accent-foreground);
      display: flex;
      align-items: center;
      justify-content: center;
      font-size: 14px;
      font-weight: 600;
      cursor: pointer;
      transition: opacity var(--duration-fast) ease;
    }

    .user-avatar:hover {
      opacity: 0.85;
    }

    /* Dropdowns */
    .dropdown {
      position: absolute;
      top: 100%;
      right: 0;
      margin-top: 8px;
      background: var(--card);
      border-radius: var(--radius-lg);
      box-shadow: 0 8px 32px rgba(0,0,0,0.24);
      z-index: 100;
      overflow: hidden;
      min-width: 200px;
    }

    .dropdown-header {
      padding: 12px 16px;
      box-shadow: 0 1px 0 var(--card-highlight);
      background: var(--bg-secondary);
    }

    .dropdown-name {
      font-size: 14px;
      font-weight: 600;
      color: var(--text);
    }

    .dropdown-email {
      font-size: 12px;
      color: var(--muted);
      margin-top: 2px;
    }

    .dropdown-status {
      display: flex;
      align-items: center;
      gap: 6px;
      margin-top: 6px;
      font-size: 11px;
      color: var(--ok);
    }

    .dropdown-status::before {
      content: '';
      width: 6px;
      height: 6px;
      border-radius: var(--radius-full);
      background: var(--ok);
    }

    .dropdown-item {
      display: flex;
      align-items: center;
      gap: 10px;
      padding: 10px 14px;
      color: var(--text);
      font-size: 14px;
      cursor: pointer;
      transition: background var(--duration-fast) ease;
    }

    .dropdown-item:hover {
      background: var(--bg-hover);
    }

    .dropdown-item.danger {
      color: var(--danger);
    }

    .dropdown-item svg {
      width: 16px;
      height: 16px;
      flex-shrink: 0;
    }

    .dropdown-divider {
      height: 1px;
      background: var(--card-highlight);
      margin: 4px 0;
    }

    /* Notification dropdown */
    .notification-dropdown {
      width: 320px;
      max-height: 400px;
    }

    .notification-header {
      display: flex;
      align-items: center;
      justify-content: space-between;
      padding: 12px 16px;
      box-shadow: 0 1px 0 var(--card-highlight);
    }

    .notification-title {
      font-size: 14px;
      font-weight: 600;
      color: var(--text);
    }

    .notification-count {
      font-size: 11px;
      color: var(--muted);
    }

    .notification-empty {
      padding: 32px 16px;
      text-align: center;
      color: var(--muted);
      font-size: 13px;
    }

    /* Responsive */
    @media (max-width: 768px) {
      .topbar-nav {
        display: none;
      }
    }
  `

  connectedCallback() {
    super.connectedCallback()
    document.addEventListener('click', this.handleDocumentClick.bind(this))
  }

  disconnectedCallback() {
    super.disconnectedCallback()
    document.removeEventListener('click', this.handleDocumentClick.bind(this))
  }

  handleDocumentClick(e: Event) {
    const target = e.target as HTMLElement
    if (!target.closest('.topbar-btn') && !target.closest('.user-avatar') && !target.closest('.dropdown')) {
      this.showUserMenu = false
    }
  }

  handleLogout() {
    import('../stores/auth-store').then(({ clearAuth }) => {
      clearAuth()
      navigate('home')
    })
  }

  getUserInitials(): string {
    const user = $user.get()
    if (!user) return '?'
    if (user.name) return user.name.charAt(0).toUpperCase()
    if (user.email) return user.email.charAt(0).toUpperCase()
    return '?'
  }

  isActive(route: string): boolean {
    return $currentRoute.get() === route
  }

  renderIcon(name: string) {
    const icons: Record<string, ReturnType<typeof html>> = {
      'bell': html`<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><path d="M14.857 17.082a24.06 24.06 0 01-8.835-2.084L3 21l1.035-3.194a24.06 24.06 0 018.835-2.084L15 15M6 6h12M6 10h12M6 14h8"/></svg>`,
      'user': html`<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><path d="M15.75 6a3.75 3.75 0 11-7.5 0 3.75 3.75 0 017.5 0zM4.501 20.118a7.5 7.5 0 0114.998 0A17.933 17.933 0 0112 21.75c-2.676 0-5.216-.584-7.499-1.632z"/></svg>`,
      'settings': html`<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><path d="M9.594 3.94c.09-.542.56-.94 1.11-.94h2.593c.55 0 1.02.398 1.11.94l.213 1.281c.063.374.313.686.645.87.074.04.147.083.22.127.324.196.72.257 1.075.124l1.217-.456a1.125 1.125 0 011.37.49l1.296 2.247a1.125 1.125 0 01-.26 1.431l-1.003.827c-.293.24-.438.613-.431.992a6.759 6.759 0 010 .255c-.007.378.138.75.43.99l1.005.828c.424.35.534.954.26 1.43l-1.298 2.247a1.125 1.125 0 01-1.369.491l-1.217-.456c-.355-.133-.75-.072-1.076.124a6.57 6.57 0 01-.22.128c-.331.183-.581.495-.644.869l-.213 1.28c-.09.543-.56.941-1.11.941h-2.594c-.55 0-1.02-.398-1.11-.94l-.213-1.281c-.062-.374-.312-.686-.644-.87a6.52 6.52 0 01-.22-.127c-.325-.196-.72-.257-1.076-.124l-1.217.456a1.125 1.125 0 01-1.369-.49l-1.297-2.247a1.125 1.125 0 01.26-1.431l1.004-.827c.292-.24.437-.613.43-.992a6.932 6.932 0 010-.255c.007-.378-.138-.75-.43-.99l-1.004-.828a1.125 1.125 0 01-.26-1.43l1.297-2.247a1.125 1.125 0 011.37-.491l1.216.456c.356.133.751.072 1.076-.124.072-.044.146-.087.22-.128.332-.183.582-.495.644-.869l.214-1.281z"/><path d="M15 12a3 3 0 11-6 0 3 3 0 016 0z"/></svg>`,
      'logout': html`<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><path d="M15.75 9V5.25A2.25 2.25 0 0013.5 3h-6a2.25 2.25 0 00-2.25 2.25v13.5A2.25 2.25 0 007.5 21h6a2.25 2.25 0 002.25-2.25V15M12 9l-3 3m0 0l3 3m-3-3h12.75"/></svg>`,
      'dashboard': html`<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><path d="M3 13.125C3 12.504 3.504 12 4.125 12h2.25c.621 0 1.125.504 1.125 1.125v6.75C7.5 20.496 6.996 21 6.375 21h-2.25A1.125 1.125 0 013 19.875v-6.75zM9.75 8.625c0-.621.504-1.125 1.125-1.125h2.25c.621 0 1.125.504 1.125 1.125v11.25c0 .621-.504 1.125-1.125 1.125h-2.25a1.125 1.125 0 01-1.125-1.125V8.625zM16.5 4.125c0-.621.504-1.125 1.125-1.125h2.25C20.496 3 21 3.504 21 4.125v15.75c0 .621-.504 1.125-1.125 1.125h-2.25a1.125 1.125 0 01-1.125-1.125V4.125z"/></svg>`,
    }
    return icons[name] || html``
  }

  renderNavLinks() {
    return html`
      <nav class="topbar-nav">
        <a class="nav-link ${this.isActive('home') ? 'active' : ''}" @click=${() => navigate('home')}>首页</a>
        <a class="nav-link ${this.isActive('marketplace') ? 'active' : ''}" @click=${() => navigate('marketplace')}>市场</a>
        <a class="nav-link ${this.isActive('dashboard') ? 'active' : ''}" @click=${() => navigate('dashboard')}>仪表盘</a>
        <a class="nav-link ${this.isActive('devices') ? 'active' : ''}" @click=${() => navigate('devices')}>设备</a>
      </nav>
    `
  }

  renderAuthenticatedContent() {
    const user = $user.get()

    return html`
      <div class="user-section">
        <!-- Console button -->
        <a class="btn btn-primary" @click=${() => navigate('dashboard')}>控制台</a>

        <!-- User avatar -->
        <div style="position: relative;">
          <div
            class="user-avatar"
            @click=${(e: Event) => { e.stopPropagation(); this.showUserMenu = !this.showUserMenu; }}
            title="${user?.name || user?.email || '用户'}"
          >
            ${this.getUserInitials()}
          </div>
          ${this.showUserMenu ? html`
            <div class="dropdown" @click=${(e: Event) => e.stopPropagation()}>
              <div class="dropdown-header">
                <div class="dropdown-name">${user?.name || '管理员'}</div>
                <div class="dropdown-email">${user?.email || 'admin@tinyiothub.com'}</div>
                <div class="dropdown-status">在线</div>
              </div>
              <div class="dropdown-item" @click=${() => { navigate('settings'); this.showUserMenu = false; }}>
                ${this.renderIcon('user')}
                <span>个人资料</span>
              </div>
              <div class="dropdown-item" @click=${() => { navigate('settings'); this.showUserMenu = false; }}>
                ${this.renderIcon('settings')}
                <span>设置</span>
              </div>
              <div class="dropdown-divider"></div>
              <div class="dropdown-item danger" @click=${() => { this.handleLogout(); this.showUserMenu = false; }}>
                ${this.renderIcon('logout')}
                <span>退出登录</span>
              </div>
            </div>
          ` : ''}
        </div>
      </div>
    `
  }

  renderUnauthenticatedContent() {
    return html`
      <div class="auth-buttons">
        <a class="btn btn-ghost" @click=${() => navigate('signin')}>登录</a>
        <a class="btn btn-primary" @click=${() => navigate('register')}>注册</a>
      </div>
    `
  }

  render() {
    const isAuthenticated = $isAuthenticated.get()

    return html`
      <header class="topbar">
        <!-- Left: Logo + Nav -->
        <div class="topbar-left">
          <a class="logo-link" @click=${() => navigate('home')}>
            <logo-icon size="32px"></logo-icon>
            <span class="logo-text">TinyIoTHub</span>
          </a>
          ${!this.backend ? this.renderNavLinks() : ''}
        </div>

        <!-- Right: Auth buttons OR User menu -->
        <div class="topbar-right">
          ${isAuthenticated ? this.renderAuthenticatedContent() : this.renderUnauthenticatedContent()}
        </div>
      </header>
    `
  }
}

declare global {
  interface HTMLElementTagNameMap {
    'app-header': AppHeader
  }
}
