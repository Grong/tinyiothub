import { LitElement, html, nothing } from "lit";
import { customElement, state } from "lit/decorators.js";
import { ApiClient, getAuthToken } from "../api/client.js";
import { loadSettings, saveSettings, type UiSettings } from "./storage.js";
import { startThemeTransition, type ThemeTransitionContext } from "./theme-transition.js";
import { resolveTheme, type ThemeMode, type ResolvedTheme } from "./theme.js";
import "./components/theme-toggle.js";
import "./components/toast.js";
import { error as toastError } from "./components/toast.js";
import "./components/skeleton.js";
import { deviceCache } from "../stores/device-cache.js";

// Views — side-effect imports register custom elements
import "./views/login.js";
import "./views/register.js";
import "./views/home.js";
import "./views/dashboard.js";
import "./views/devices.js";
import "./views/alarms.js";
import "./views/events.js";
import "./views/monitoring.js";
import "./views/templates.js";
import "./views/drivers.js";
import "./views/tags.js";
import "./views/users.js";
import "./views/settings.js";
import "./views/chat.js";
import "./views/agents.js";
import "./views/cron.js";

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
      { route: "dashboard", label: "概览", icon: "M3 3h7v7H3zM14 3h7v7h-7zM14 14h7v7h-7zM3 14h7v7H3z" },
    ],
  },
  {
    label: "设备管理",
    items: [
      { route: "devices", label: "设备列表", icon: "M21 16V8a2 2 0 0 0-1-1.73l-7-4a2 2 0 0 0-2 0l-7 4A2 2 0 0 0 3 8v8a2 2 0 0 0 1 1.73l7 4a2 2 0 0 0 2 0l7-4A2 2 0 0 0 21 16z" },
      { route: "templates", label: "设备模板", icon: "M14 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V8z" },
      { route: "drivers", label: "驱动管理", icon: "M12 2L2 7l10 5 10-5-10-5zM2 17l10 5 10-5M2 12l10 5 10-5" },
    ],
  },
  {
    label: "监控告警",
    items: [
      { route: "alarms", label: "告警中心", icon: "M10.29 3.86L1.82 18a2 2 0 0 0 1.71 3h16.94a2 2 0 0 0 1.71-3L13.71 3.86a2 2 0 0 0-3.42 0zM12 9v4M12 17h.01" },
      { route: "events", label: "事件日志", icon: "M14 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V8z" },
      { route: "monitoring", label: "系统监控", icon: "M18 20V10M12 20V4M6 20v-6" },
      { route: "cron", label: "定时任务", icon: "M12 2a10 10 0 1 0 10 10A10 10 0 0 0 12 2zm0 18a8 8 0 1 1 8-8 8 8 0 0 1-8 8zm1-8h4v2H11V6h2z" },
    ],
  },
  {
    label: "AI 助手",
    items: [
      { route: "chat", label: "AI 聊天", icon: "M21 15a2 2 0 0 1-2 2H7l-4 4V5a2 2 0 0 1 2-2h14a2 2 0 0 1 2 2z" },
      { route: "agents", label: "Agent 管理", icon: "M12 5a3 3 0 1 0-5.997.125 4 4 0 0 0-2.526 5.77 4 4 0 0 0 .556 6.588A4 4 0 1 0 12 18Z" },
    ],
  },
  {
    label: "系统管理",
    items: [
      { route: "tags", label: "标签管理", icon: "M20.59 13.41l-7.17 7.17a2 2 0 0 1-2.83 0L2 12V2h10l8.59 8.59a2 2 0 0 1 0 2.82zM7 7h.01" },
      { route: "users", label: "用户管理", icon: "M17 21v-2a4 4 0 0 0-4-4H5a4 4 0 0 0-4 4v2M9 7a4 4 0 1 0 0-8 4 4 0 0 0 0 8zM23 21v-2a4 4 0 0 0-3-3.87M16 3.13a4 4 0 0 1 0 7.75", adminOnly: true },
      { route: "settings", label: "系统设置", icon: "M12 15a3 3 0 1 0 0-6 3 3 0 0 0 0 6zM19.4 15a1.65 1.65 0 0 0 .33 1.82l.06.06a2 2 0 0 1 0 2.83 2 2 0 0 1-2.83 0l-.06-.06a1.65 1.65 0 0 0-1.82-.33 1.65 1.65 0 0 0-1 1.51V21a2 2 0 0 1-4 0v-.09A1.65 1.65 0 0 0 9 19.4a1.65 1.65 0 0 0-1.82.33l-.06.06a2 2 0 0 1-2.83 0 2 2 0 0 1 0-2.83l.06-.06A1.65 1.65 0 0 0 4.68 15a1.65 1.65 0 0 0-1.51-1H3a2 2 0 0 1 0-4h.09A1.65 1.65 0 0 0 4.6 9a1.65 1.65 0 0 0-.33-1.82l-.06-.06a2 2 0 0 1 0-2.83 2 2 0 0 1 2.83 0l.06.06A1.65 1.65 0 0 0 9 4.68a1.65 1.65 0 0 0 1-1.51V3a2 2 0 0 1 4 0v.09a1.65 1.65 0 0 0 1 1.51 1.65 1.65 0 0 0 1.82-.33l.06-.06a2 2 0 0 1 2.83 0 2 2 0 0 1 0 2.83l-.06.06A1.65 1.65 0 0 0 19.4 9a1.65 1.65 0 0 0 1.51 1H21a2 2 0 0 1 0 4h-.09a1.65 1.65 0 0 0-1.51 1z" },
    ],
  },
];

@customElement("tinyiothub-app")
export class TinyIoTHubApp extends LitElement {
  @state() settings: UiSettings = loadSettings();
  @state() currentRoute = "login";
  @state() isAuthenticated = false;
  @state() navCollapsed = false;
  @state() theme: ThemeMode = this.settings.theme ?? "system";
  @state() resolvedTheme: ResolvedTheme = "dark";
  @state() userMenuOpen = false;
  @state() userName = "";
  @state() userAvatar = "U";
  @state() userRole = "";

  private themeMediaQuery: MediaQueryList | null = null;
  private themeChangeHandler = () => {
    if (this.theme === "system") {
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
    window.addEventListener("auth-error", this.handleAuthError);
    if (this.isAuthenticated) {
      this.loadUserInfo();
    }
  }

  disconnectedCallback() {
    super.disconnectedCallback();
    window.removeEventListener("popstate", this.boundHandleRoute);
    window.removeEventListener("auth-error", this.handleAuthError);
    if (this.themeMediaQuery) {
      this.themeMediaQuery.removeEventListener("change", this.themeChangeHandler);
    }
  }

  // --- Theme ---

  loadTheme() {
    this.theme = this.settings.theme ?? "system";
    this.updateResolvedTheme();
    this.applyTheme();
    if (typeof window !== "undefined" && window.matchMedia) {
      this.themeMediaQuery = window.matchMedia("(prefers-color-scheme: dark)");
      this.themeMediaQuery.addEventListener("change", this.themeChangeHandler);
    }
  }

  updateResolvedTheme() {
    this.resolvedTheme = resolveTheme(this.theme);
  }

  applyTheme() {
    if (this.resolvedTheme === "light") {
      document.documentElement.setAttribute("data-theme", "light");
    } else {
      document.documentElement.removeAttribute("data-theme");
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
      const res = await ApiClient.get<any>("/users/me");
      const user = res.result;
      if (user) {
        this.userName = user.name || user.username || "用户";
        this.userAvatar = this.userName.charAt(0).toUpperCase();
        this.userRole = user.role || "";
      }
    } catch {
      // ignore
    }
  }

  logout() {
    sessionStorage.removeItem("auth-token");
    localStorage.removeItem("auth-token");
    deviceCache.clearCache();
    this.isAuthenticated = false;
    toastError("登录已过期，请重新登录");
    this.navigate("login");
  }

  // --- Router ---

  setupRouter() {
    window.addEventListener("popstate", this.boundHandleRoute);
    this.handleRoute();
  }

  handleRoute() {
    const path = window.location.pathname.slice(1) || "";

    // Handle /devices/:id
    if (path.startsWith("devices/")) {
      this.currentRoute = path;
      return;
    }

    this.currentRoute = path || "home";

    const publicRoutes = ["login", "register", "home", ""];
    if (!publicRoutes.includes(path) && !this.isAuthenticated) {
      this.navigate("login");
    }
  }

  navigate(route: string) {
    window.history.pushState({}, "", `/${route}`);
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
      dashboard: "概览",
      devices: "设备管理",
      templates: "设备模板",
      drivers: "驱动管理",
      alarms: "告警中心",
      events: "事件日志",
      monitoring: "系统监控",
      tags: "标签管理",
      users: "用户管理",
      settings: "系统设置",
      marketplace: "市场",
      chat: "AI 聊天",
      agents: "Agent 管理",
      cron: "定时任务",
    };
    // Handle /devices/:id
    if (this.currentRoute.startsWith("devices/")) return "设备详情";
    return titles[this.currentRoute] || "";
  }

  getPageSubtitle(): string {
    const subs: Record<string, string> = {
      dashboard: "设备状态、告警概览和系统指标",
      devices: "管理所有接入的 IoT 设备",
      templates: "管理设备模板和物模型",
      drivers: "管理协议驱动（Modbus、ONVIF、SNMP、MQTT）",
      alarms: "查看和管理设备告警",
      events: "查看设备事件日志",
      monitoring: "系统资源和性能监控",
      tags: "管理设备标签和分组",
      users: "管理系统用户和权限",
      settings: "系统配置和参数管理",
      marketplace: "驱动和模板市场",
      chat: "与 AI Agent 对话",
      agents: "管理和配置 Agent",
      cron: "管理定时执行的任务和作业",
    };
    if (this.currentRoute.startsWith("devices/")) return "查看设备属性、命令和事件";
    return subs[this.currentRoute] || "";
  }

  // --- Render ---

  render() {
    // Login & Home — no shell
    if (this.currentRoute === "login") {
      return html`<view-login></view-login>`;
    }

    if (this.currentRoute === "register") {
      return html`<view-register></view-register>`;
    }

    if (this.currentRoute === "home") {
      return html`<view-home></view-home>`;
    }

    // All other routes require auth
    if (!this.isAuthenticated) {
      return html`<view-login></view-login>`;
    }

    return html`
      <div class="shell">
        <div class="topbar" role="banner">
        ${this.renderTopbarInner()}
        </div>
        <nav class="nav ${this.navCollapsed ? "nav--collapsed" : ""}" aria-label="主导航" role="navigation">
          ${this.renderNav()}
        </nav>
        <div class="content" role="main" id="main-content">

          ${this.currentRoute.startsWith("devices/") || this.currentRoute === "chat" ? nothing : html`
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
          <a href="/" class="brand" @click=${(e: Event) => { e.preventDefault(); this.navigate("dashboard"); }}
            style="cursor: pointer; text-decoration: none;">
            <img src="/logo.svg" alt="TinyIoTHub" style="width: 36px; height: 36px;" onerror="this.style.display='none'" />
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
            ${this.userMenuOpen ? html`
              <div class="user-dropdown">
                <div class="user-dropdown-header">
                  <span class="user-avatar user-avatar--lg">${this.userAvatar}</span>
                  <div class="user-info">
                    <div class="user-name">${this.userName || "用户"}</div>
                  </div>
                </div>
                <div class="user-dropdown-divider"></div>
                <button @click=${() => { this.navigate("dashboard"); this.closeUserMenu(); }} class="user-dropdown-item">
                  概览
                </button>
                <button @click=${() => { this.navigate("settings"); this.closeUserMenu(); }} class="user-dropdown-item">
                  系统设置
                </button>
                <button @click=${() => { this.logout(); this.closeUserMenu(); }} class="user-dropdown-item user-dropdown-item--danger">
                  退出登录
                </button>
              </div>
            ` : ""}
        </div>
    `;
  }

  renderNav() {
    return html`
      ${NAV_GROUPS.map(group => {
        const visibleItems = group.items.filter(item => !item.adminOnly || this.userRole === "admin");
        if (visibleItems.length === 0) return "";
        return html`
          <div class="nav-group">
            ${group.label ? html`
              <div class="nav-label nav-label--static">
                <span class="nav-label__text">${group.label}</span>
              </div>
            ` : ""}
            <div class="nav-group__items">
              ${visibleItems.map(item => html`
                <a href="/${item.route}"
                  class="nav-item ${this.currentRoute === item.route || (item.route === "devices" && this.currentRoute.startsWith("devices/")) ? "active" : ""}"
                  @click=${(e: Event) => { e.preventDefault(); this.navigate(item.route); }}
                >
                  <span class="nav-item__icon">
                    <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
                      <path d="${item.icon}"></path>
                    </svg>
                  </span>
                  <span class="nav-item__text">${item.label}</span>
                </a>
              `)}
            </div>
          </div>
        `;
      })}
    `;
  }

  renderPage() {
    const route = this.currentRoute;
    if (route === "dashboard") return html`<view-dashboard></view-dashboard>`;
    if (route === "devices" || route.startsWith("devices/")) return html`<view-devices></view-devices>`;
    if (route === "alarms") return html`<view-alarms></view-alarms>`;
    if (route === "events") return html`<view-events></view-events>`;
    if (route === "monitoring") return html`<view-monitoring></view-monitoring>`;
    if (route === "templates") return html`<view-templates></view-templates>`;
    if (route === "drivers") return html`<view-drivers></view-drivers>`;
    if (route === "tags") return html`<view-tags></view-tags>`;
    if (route === "users") return html`<view-users></view-users>`;
    if (route === "settings") return html`<view-settings></view-settings>`;
    if (route === "chat") return html`<view-chat></view-chat>`;
    if (route === "agents") return html`<view-agents></view-agents>`;
    if (route === "cron") return html`<view-cron></view-cron>`;
    return html`<div style="padding: 40px; text-align: center; color: var(--muted);">页面不存在</div>`;
  }
}
