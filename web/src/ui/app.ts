import { LitElement, html, nothing } from 'lit';
import { customElement, state } from 'lit/decorators.js';
import { ApiClient, getAuthToken } from '../api/client.js';
import { loadSettings, saveSettings, type UiSettings } from './storage.js';
import { startThemeTransition, type ThemeTransitionContext } from './theme-transition.js';
import { resolveTheme, type ThemeMode, type ResolvedTheme } from './theme.js';
import './components/theme-toggle.js';
import './components/toast.js';
import { error as toastError } from './components/toast.js';
import { deviceCache } from '../stores/device-cache.js';

// Public / first-screen views — eagerly loaded
import './views/login.js';
import './views/register.js';
import './views/home.js';

// Lazy view loaders — each returns a Promise that resolves once the
// custom element is registered.
const lazyViews: Record<string, () => Promise<void>> = {
  dashboard: () => import('./views/dashboard.js').then(() => {}),
  devices: () => import('./views/devices.js').then(() => {}),
  alarms: () => import('./views/alarms.js').then(() => {}),
  events: () => import('./views/events.js').then(() => {}),
  monitoring: () => import('./views/monitoring.js').then(() => {}),
  'local-resources': () => import('./views/local-resources.js').then(() => {}),
  users: () => import('./views/users.js').then(() => {}),
  settings: () => import('./views/settings.js').then(() => {}),
  chat: () => import('./views/chat.js').then(() => {}),
  agents: () => import('./views/agents.js').then(() => {}),
  cron: () => import('./views/cron.js').then(() => {}),
  terms: () => import('./views/terms.js').then(() => {}),
  privacy: () => import('./views/privacy.js').then(() => {}),
  marketplace: () => import('./views/marketplace.js').then(() => {}),
  'driver-health': () => import('./views/driver-health.js').then(() => {}),
  'memory-dashboard': () => import('./views/memory-dashboard.js').then(() => {}),
  knowledge: () => import('./views/knowledge.js').then(() => {}),
};

interface NavItem {
  route: string;
  label: string;
  icon: string;
  adminOnly?: boolean;
}

interface NavGroup {
  label?: string;
  items: NavItem[];
}

const NAV_GROUPS: NavGroup[] = [
  {
    items: [
      {
        route: 'chat',
        label: 'AI 助手',
        icon: 'M21 15a2 2 0 0 1-2 2H7l-4 4V5a2 2 0 0 1 2-2h14a2 2 0 0 1 2 2z',
      },
      {
        route: 'dashboard',
        label: '概览',
        icon: 'M3 3h7v7H3zM14 3h7v7h-7zM14 14h7v7h-7zM3 14h7v7H3z',
      },
    ],
  },
  {
    label: '设备管理',
    items: [
      {
        route: 'devices',
        label: '设备列表',
        icon: 'M21 16V8a2 2 0 0 0-1-1.73l-7-4a2 2 0 0 0-2 0l-7 4A2 2 0 0 0 3 8v8a2 2 0 0 0 1 1.73l7 4a2 2 0 0 0 2 0l7-4A2 2 0 0 0 21 16z',
      },
      {
        route: 'local-resources',
        label: '本地资源',
        icon: 'M4 21v-7M4 10V3M12 21v-9M12 8V3M20 21v-5M20 12V3M1 14h6M9 8h6M17 16h6',
      },
      {
        route: 'marketplace',
        label: '应用市场',
        icon: 'M3 3h18v18H3V3zm4 4v10h4V7H7zm6 0v10h4V7h-4z',
      },
    ],
  },
  {
    label: '监控告警',
    items: [
      {
        route: 'alarms',
        label: '告警中心',
        icon: 'M10.29 3.86L1.82 18a2 2 0 0 0 1.71 3h16.94a2 2 0 0 0 1.71-3L13.71 3.86a2 2 0 0 0-3.42 0zM12 9v4M12 17h.01',
      },
      {
        route: 'events',
        label: '事件日志',
        icon: 'M14 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V8z',
      },
      { route: 'monitoring', label: '系统监控', icon: 'M18 20V10M12 20V4M6 20v-6' },
      { route: 'driver-health', label: '驱动健康', icon: 'M22 12h-4l-3 9L9 3l-3 9H2' },
      {
        route: 'cron',
        label: '定时任务',
        icon: 'M12 2a10 10 0 1 0 10 10A10 10 0 0 0 12 2zm0 18a8 8 0 1 1 8-8 8 8 0 0 1-8 8zm1-8h4v2H11V6h2z',
      },
    ],
  },
  {
    label: '智能管理',
    items: [
      {
        route: 'agents',
        label: 'Agent 管理',
        icon: 'M12 5a3 3 0 1 0-5.997.125 4 4 0 0 0-2.526 5.77 4 4 0 0 0 .556 6.588A4 4 0 1 0 12 18Z',
      },
      {
        route: 'memory-dashboard',
        label: '记忆面板',
        icon: 'M4 4h16a2 2 0 0 1 2 2v12a2 2 0 0 1-2 2H4a2 2 0 0 1-2-2V6a2 2 0 0 1 2-2zm0 4v8h16V8H4zm3 2h4v4H7v-4zm6 0h4v2h-4v-2z',
      },
      {
        route: 'knowledge',
        label: '知识图谱',
        icon: 'M5 3a2 2 0 1 0 0 4 2 2 0 0 0 0-4z M19 3a2 2 0 1 0 0 4 2 2 0 0 0 0-4z M12 17a2 2 0 1 0 0 4 2 2 0 0 0 0-4z M7 7l5 8 M17 7l-5 8',
      },
    ],
  },
  {
    label: '系统管理',
    items: [
      {
        route: 'users',
        label: '用户管理',
        icon: 'M17 21v-2a4 4 0 0 0-4-4H5a4 4 0 0 0-4 4v2M9 7a4 4 0 1 0 0-8 4 4 0 0 0 0 8zM23 21v-2a4 4 0 0 0-3-3.87M16 3.13a4 4 0 0 1 0 7.75',
        adminOnly: true,
      },
      {
        route: 'settings',
        label: '系统设置',
        icon: 'M12 15a3 3 0 1 0 0-6 3 3 0 0 0 0 6zM19.4 15a1.65 1.65 0 0 0 .33 1.82l.06.06a2 2 0 0 1 0 2.83 2 2 0 0 1-2.83 0l-.06-.06a1.65 1.65 0 0 0-1.82-.33 1.65 1.65 0 0 0-1 1.51V21a2 2 0 0 1-4 0v-.09A1.65 1.65 0 0 0 9 19.4a1.65 1.65 0 0 0-1.82.33l-.06.06a2 2 0 0 1-2.83 0 2 2 0 0 1 0-2.83l.06-.06A1.65 1.65 0 0 0 4.68 15a1.65 1.65 0 0 0-1.51-1H3a2 2 0 0 1 0-4h.09A1.65 1.65 0 0 0 4.6 9a1.65 1.65 0 0 0-.33-1.82l-.06-.06a2 2 0 0 1 0-2.83 2 2 0 0 1 2.83 0l.06.06A1.65 1.65 0 0 0 9 4.68a1.65 1.65 0 0 0 1-1.51V3a2 2 0 0 1 4 0v.09a1.65 1.65 0 0 0 1 1.51 1.65 1.65 0 0 0 1.82-.33l.06-.06a2 2 0 0 1 2.83 0 2 2 0 0 1 0 2.83l-.06.06A1.65 1.65 0 0 0 19.4 9a1.65 1.65 0 0 0 1.51 1H21a2 2 0 0 1 0 4h-.09a1.65 1.65 0 0 0-1.51 1z',
      },
    ],
  },
];

@customElement('tinyiothub-app')
export class TinyIoTHubApp extends LitElement {
  @state() settings: UiSettings = loadSettings();
  @state() currentRoute = 'login';
  @state() isAuthenticated = false;
  @state() navCollapsed = false;
  @state() theme: ThemeMode = this.settings.theme ?? 'system';
  @state() resolvedTheme: ResolvedTheme = 'dark';
  @state() userMenuOpen = false;
  @state() userName = '';
  @state() userAvatar = 'U';
  @state() userRole = '';
  @state() loadingRoute: string | null = null;
  @state() loadError: string | null = null;

  private loadSeq = 0;
  private themeMediaQuery: MediaQueryList | null = null;
  private themeChangeHandler = () => {
    if (this.theme === 'system') {
      this.updateResolvedTheme();
      this.applyTheme();
    }
  };
  private boundHandleRoute = () => this.handleRoute();
  private handleAuthError = () => this.logout();

  createRenderRoot() {
    return this;
  }

  connectedCallback() {
    super.connectedCallback();
    this.loadTheme();
    this.checkAuth();
    this.setupRouter();
    window.addEventListener('auth-error', this.handleAuthError);
    if (this.isAuthenticated) {
      this.loadUserInfo();
    }
    // Notify skeleton screen that app has bootstrapped
    requestAnimationFrame(() => {
      document.documentElement.dispatchEvent(new CustomEvent('app-ready'));
    });
  }

  disconnectedCallback() {
    super.disconnectedCallback();
    window.removeEventListener('popstate', this.boundHandleRoute);
    window.removeEventListener('auth-error', this.handleAuthError);
    if (this.themeMediaQuery) {
      this.themeMediaQuery.removeEventListener('change', this.themeChangeHandler);
    }
  }

  // --- Theme ---

  loadTheme() {
    this.theme = this.settings.theme ?? 'system';
    this.updateResolvedTheme();
    this.applyTheme();
    if (typeof window !== 'undefined' && window.matchMedia) {
      this.themeMediaQuery = window.matchMedia('(prefers-color-scheme: dark)');
      this.themeMediaQuery.addEventListener('change', this.themeChangeHandler);
    }
  }

  updateResolvedTheme() {
    this.resolvedTheme = resolveTheme(this.theme);
  }

  applyTheme() {
    if (this.resolvedTheme === 'light') {
      document.documentElement.setAttribute('data-theme', 'light');
    } else {
      document.documentElement.removeAttribute('data-theme');
    }
  }

  handleThemeChange(event: CustomEvent) {
    const { theme, event: mouseEvent } = event.detail;
    this.setTheme(theme, mouseEvent);
  }

  setTheme(nextTheme: ThemeMode, mouseEvent?: MouseEvent) {
    if (this.theme === nextTheme) return;
    const apply = () => {
      this.theme = nextTheme;
      this.settings = { ...this.settings, theme: nextTheme };
      saveSettings(this.settings);
      this.updateResolvedTheme();
      this.applyTheme();
    };
    const context: ThemeTransitionContext = {};
    if (mouseEvent?.clientX != null && mouseEvent?.clientY != null) {
      context.pointerClientX = mouseEvent.clientX;
      context.pointerClientY = mouseEvent.clientY;
    }
    startThemeTransition({ nextTheme, applyTheme: apply, context, currentTheme: this.theme });
  }

  // --- Auth ---

  checkAuth() {
    const token = getAuthToken();
    this.isAuthenticated = !!token;
  }

  async loadUserInfo() {
    try {
      const res = await ApiClient.get<any>('/users/me');
      const user = res.result;
      if (user) {
        this.userName = user.name || user.username || '用户';
        this.userAvatar = this.userName.charAt(0).toUpperCase();
        this.userRole = user.role || '';
      }
    } catch {
      // ignore
    }
  }

  logout() {
    sessionStorage.removeItem('auth-token');
    localStorage.removeItem('auth-token');
    deviceCache.clearCache();
    this.isAuthenticated = false;
    toastError('登录已过期，请重新登录');
    this.navigate('login');
  }

  // --- Router ---

  setupRouter() {
    window.addEventListener('popstate', this.boundHandleRoute);
    this.handleRoute();
  }

  handleRoute() {
    const path = window.location.pathname.slice(1) || '';

    // Handle /devices/:id
    if (path.startsWith('devices/')) {
      this.currentRoute = path;
      this._ensureViewLoaded('devices');
      return;
    }

    this.currentRoute = path || 'chat';

    const publicRoutes = ['login', 'register', 'home', 'terms', 'privacy', ''];
    if (!publicRoutes.includes(path) && !this.isAuthenticated) {
      this.navigate('login');
      return;
    }

    // Authenticated users landing on / get redirected to chat
    if (!path && this.isAuthenticated) {
      this.navigate('chat');
      return;
    }

    this._ensureViewLoaded(this.currentRoute);
  }

  /** Lazy-load the view module for the given route if needed. */
  private _ensureViewLoaded(route: string) {
    const base = route.startsWith('devices/') ? 'devices' : route;
    const loader = lazyViews[base];
    if (!loader) return;
    const tag = `view-${base}`;
    if (customElements.get(tag)) return; // already registered
    this.loadError = null;
    this.loadingRoute = base;
    const seq = ++this.loadSeq;
    loader()
      .then(() => {
        if (this.loadSeq === seq) {
          this.loadingRoute = null;
        }
      })
      .catch((err) => {
        if (this.loadSeq === seq) {
          this.loadingRoute = null;
          this.loadError = `加载页面失败: ${base}`;
          console.error(`Lazy load failed for ${base}:`, err);
        }
      });
  }

  navigate(route: string) {
    window.history.pushState({}, '', `/${route}`);
    this.handleRoute();
  }

  toggleNav() {
    this.navCollapsed = !this.navCollapsed;
  }

  toggleUserMenu() {
    this.userMenuOpen = !this.userMenuOpen;
  }

  closeUserMenu() {
    this.userMenuOpen = false;
  }

  getPageTitle(): string {
    const titles: Record<string, string> = {
      dashboard: '概览',
      devices: '设备管理',
      'local-resources': '本地资源',
      alarms: '告警中心',
      events: '事件日志',
      monitoring: '系统监控',
      users: '用户管理',
      settings: '系统设置',
      marketplace: '应用市场',
      knowledge: '知识图谱',
      'driver-health': '驱动健康',
      chat: 'AI 聊天',
      agents: 'Agent 管理',
      cron: '定时任务',
      'memory-dashboard': '记忆面板',
    };
    // Handle /devices/:id
    if (this.currentRoute.startsWith('devices/')) return '设备详情';
    return titles[this.currentRoute] || '';
  }

  getPageSubtitle(): string {
    const subs: Record<string, string> = {
      dashboard: '设备状态、告警概览和系统指标',
      devices: '管理所有接入的 IoT 设备',
      'local-resources': '管理设备模板和协议驱动',
      alarms: '查看和管理设备告警',
      events: '查看设备事件日志',
      monitoring: '系统资源和性能监控',
      users: '管理系统用户和权限',
      settings: '系统配置和参数管理',
      marketplace: '浏览和安装模板与驱动',
      knowledge: '管理知识文档，构建物联网场景知识图谱',
      'driver-health': '查看已加载动态驱动的运行状态',
      chat: '与 AI Agent 对话',
      agents: '管理和配置 Agent',
      cron: '管理定时执行的任务和作业',
      'memory-dashboard': '查看和管理 Agent 记忆与反思队列',
    };
    if (this.currentRoute.startsWith('devices/')) return '查看设备属性、命令和事件';
    return subs[this.currentRoute] || '';
  }

  // --- Render ---

  render() {
    // Login & Home — no shell
    if (this.currentRoute === 'login') {
      return html`<view-login></view-login>`;
    }

    if (this.currentRoute === 'register') {
      return html`<view-register></view-register>`;
    }

    if (this.currentRoute === 'home') {
      return html`<view-home></view-home>`;
    }

    if (this.currentRoute === 'terms') {
      return html`<view-terms></view-terms>`;
    }

    if (this.currentRoute === 'privacy') {
      return html`<view-privacy></view-privacy>`;
    }

    // All other routes require auth
    if (!this.isAuthenticated) {
      return html`<view-login></view-login>`;
    }

    return html`
      <div class="shell">
        <div class="topbar" role="banner">${this.renderTopbarInner()}</div>
        <nav
          class="nav ${this.navCollapsed ? 'nav--collapsed' : ''}"
          aria-label="主导航"
          role="navigation"
        >
          ${this.renderNav()}
        </nav>
        <div class="content" role="main" id="main-content">
          ${this.currentRoute.startsWith('devices/') || this.currentRoute === 'chat'
            ? nothing
            : html`
                <section class="content-header">
                  <div>
                    <div class="page-title">${this.getPageTitle()}</div>
                    <div class="page-sub">${this.getPageSubtitle()}</div>
                  </div>
                </section>
              `}
          ${this.renderPage()}
        </div>
        <toast-container></toast-container>
      </div>
    `;
  }

  renderTopbarInner() {
    return html`
      <div class="topbar-left">
        <a
          href="/"
          class="brand"
          @click=${(e: Event) => {
            e.preventDefault();
            this.navigate('chat');
          }}
          style="cursor: pointer; text-decoration: none;"
        >
          <img
            src="/logo.svg"
            alt="TinyIoTHub"
            style="width: 36px; height: 36px;"
            onerror="this.style.display='none'"
          />
          <div class="brand-text">
            <div class="brand-title">TinyIoTHub</div>
          </div>
        </a>
      </div>
      <div class="topbar-right">
        <theme-toggle .theme=${this.theme} @theme-change=${this.handleThemeChange}></theme-toggle>
        <div class="user-menu-container">
          <button class="user-avatar-btn" @click=${this.toggleUserMenu} aria-label="用户菜单">
            <span class="user-avatar">${this.userAvatar}</span>
          </button>
          ${this.userMenuOpen
            ? html`
                <div class="user-dropdown">
                  <div class="user-dropdown-header">
                    <span class="user-avatar user-avatar--lg">${this.userAvatar}</span>
                    <div class="user-info">
                      <div class="user-name">${this.userName || '用户'}</div>
                    </div>
                  </div>
                  <div class="user-dropdown-divider"></div>
                  <button
                    @click=${() => {
                      this.navigate('dashboard');
                      this.closeUserMenu();
                    }}
                    class="user-dropdown-item"
                  >
                    概览
                  </button>
                  <button
                    @click=${() => {
                      this.navigate('settings');
                      this.closeUserMenu();
                    }}
                    class="user-dropdown-item"
                  >
                    系统设置
                  </button>
                  <button
                    @click=${() => {
                      this.logout();
                      this.closeUserMenu();
                    }}
                    class="user-dropdown-item user-dropdown-item--danger"
                  >
                    退出登录
                  </button>
                </div>
              `
            : ''}
        </div>
      </div>
    `;
  }

  renderNav() {
    return html`
      ${NAV_GROUPS.map((group) => {
        const visibleItems = group.items.filter(
          (item) => !item.adminOnly || this.userRole === 'admin',
        );
        if (visibleItems.length === 0) return '';
        return html`
          <div class="nav-group">
            ${group.label
              ? html`
                  <div class="nav-label nav-label--static">
                    <span class="nav-label__text">${group.label}</span>
                  </div>
                `
              : ''}
            <div class="nav-group__items">
              ${visibleItems.map(
                (item) => html`
                  <a
                    href="/${item.route}"
                    class="nav-item ${this.currentRoute === item.route ||
                    (item.route === 'devices' && this.currentRoute.startsWith('devices/'))
                      ? 'active'
                      : ''}"
                    @click=${(e: Event) => {
                      e.preventDefault();
                      this.navigate(item.route);
                    }}
                  >
                    <span class="nav-item__icon">
                      <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
                        <path d="${item.icon}"></path>
                      </svg>
                    </span>
                    <span class="nav-item__text">${item.label}</span>
                  </a>
                `,
              )}
            </div>
          </div>
        `;
      })}
    `;
  }

  renderPage() {
    const route = this.currentRoute;
    // Eager views — always available
    if (route === 'login') return html`<view-login></view-login>`;
    if (route === 'register') return html`<view-register></view-register>`;
    if (route === 'home') return html`<view-home></view-home>`;

    const base = route.startsWith('devices/') ? 'devices' : route;
    const tag = `view-${base}`;
    const isReady = !!customElements.get(tag);
    const isLoading = this.loadingRoute === base;

    if (!isReady && isLoading) {
      return this._renderLoading();
    }

    // All lazy views use the same tag pattern
    if (base === 'dashboard') return html`<view-dashboard></view-dashboard>`;
    if (base === 'devices') return html`<view-devices></view-devices>`;
    if (base === 'alarms') return html`<view-alarms></view-alarms>`;
    if (base === 'events') return html`<view-events></view-events>`;
    if (base === 'monitoring') return html`<view-monitoring></view-monitoring>`;
    if (base === 'local-resources') return html`<view-local-resources></view-local-resources>`;
    if (base === 'users') return html`<view-users></view-users>`;
    if (base === 'settings') return html`<view-settings></view-settings>`;
    if (base === 'chat') return html`<view-chat></view-chat>`;
    if (base === 'agents') return html`<view-agents></view-agents>`;
    if (base === 'cron') return html`<view-cron></view-cron>`;
    if (base === 'marketplace') return html`<view-marketplace></view-marketplace>`;
    if (base === 'driver-health') return html`<view-driver-health></view-driver-health>`;
    if (base === 'memory-dashboard') return html`<view-memory-dashboard></view-memory-dashboard>`;
    if (base === 'knowledge') return html`<view-knowledge></view-knowledge>`;
    return html`<div style="padding: 40px; text-align: center; color: var(--muted);">
      页面不存在
    </div>`;
  }

  private _renderLoading() {
    if (this.loadError) {
      return html`
        <div
          style="display:flex;flex-direction:column;align-items:center;justify-content:center;height:60vh;color:var(--muted);font-size:14px;gap:12px"
        >
          <div style="color:var(--error)">${this.loadError}</div>
          <button
            class="btn btn--primary btn--sm"
            @click=${() => this._ensureViewLoaded(this.currentRoute)}
          >
            重试
          </button>
        </div>
      `;
    }
    return html`
      <div
        style="display:flex;align-items:center;justify-content:center;height:60vh;color:var(--muted);font-size:14px"
      >
        <span
          style="width:16px;height:16px;border:2px solid var(--border);border-top-color:var(--primary);border-radius:50%;animation:spin 1s linear infinite;display:inline-block;margin-right:8px"
        ></span>
        加载中…
      </div>
    `;
  }
}
