import { LitElement, html } from "lit";
import { customElement, state } from "lit/decorators.js";
import { getAuthToken } from "../../api/client.js";
import { loadSettings, saveSettings, type UiSettings } from "../storage.js";
import { resolveTheme, type ThemeMode, type ResolvedTheme } from "../theme.js";
import "../components/theme-toggle.js";
import "../components/point-earth.js";
import "../components/showcase-viz.js";
import "./home-panel.js";
import "./home.css";

@customElement("view-home")
export class HomeView extends LitElement {
  @state() isAuthenticated = false;
  @state() navVisible = true;
  @state() headerScrolled = false;
  @state() settings: UiSettings = loadSettings();
  @state() theme: ThemeMode = this.settings.theme ?? "system";
  @state() resolvedTheme: ResolvedTheme = "dark";

  private themeMediaQuery: MediaQueryList | null = null;
  private themeChangeHandler = () => {
    if (this.theme === "system") {
      this.updateResolvedTheme();
      this.applyTheme();
    }
  };
  private lastScrollY = 0;
  private scrollHandler = () => {
    const y = window.scrollY;
    if (y > this.lastScrollY && y > 80) {
      this.navVisible = false;
    } else {
      this.navVisible = true;
    }
    this.headerScrolled = y > 20;
    this.lastScrollY = y;
  };
  private revealObserver: IntersectionObserver | null = null;

  createRenderRoot() {
    return this;
  }

  connectedCallback() {
    super.connectedCallback();
    const token = getAuthToken();
    this.isAuthenticated = !!token;
    window.addEventListener("scroll", this.scrollHandler, { passive: true });
    this.loadTheme();
    this.initRevealObserver();
  }

  disconnectedCallback() {
    super.disconnectedCallback();
    window.removeEventListener("scroll", this.scrollHandler);
    if (this.themeMediaQuery) {
      this.themeMediaQuery.removeEventListener("change", this.themeChangeHandler);
    }
    if (this.revealObserver) {
      this.revealObserver.disconnect();
      this.revealObserver = null;
    }
  }

  private initRevealObserver() {
    if (typeof window === "undefined" || !("IntersectionObserver" in window)) {
      // Fallback: make everything visible immediately
      this.querySelectorAll(".reveal").forEach((el) => el.classList.add("is-visible"));
      return;
    }
    this.revealObserver = new IntersectionObserver(
      (entries) => {
        entries.forEach((entry) => {
          if (entry.isIntersecting) {
            entry.target.classList.add("is-visible");
            this.revealObserver?.unobserve(entry.target);
          }
        });
      },
      { threshold: 0.1, rootMargin: "0px 0px -50px 0px" }
    );
    // Observe after a microtask so DOM is ready
    queueMicrotask(() => {
      this.querySelectorAll(".reveal").forEach((el) => {
        this.revealObserver?.observe(el);
      });
    });
  }

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
    const { theme } = event.detail;
    this.theme = theme;
    this.settings = { ...this.settings, theme };
    saveSettings(this.settings);
    this.updateResolvedTheme();
    this.applyTheme();
  }

  navigate(path: string) {
    window.history.pushState({}, "", path);
    window.dispatchEvent(new PopStateEvent("popstate"));
  }

  render() {
    return html`
      <div class="home">
        <!-- Shared gradient defs for icons -->
        <svg style="position:absolute;width:0;height:0;">
          <defs>
            <linearGradient id="icon-grad" x1="0%" y1="0%" x2="100%" y2="100%">
              <stop offset="0%" stop-color="#00d4ff" />
              <stop offset="100%" stop-color="#7b61ff" />
            </linearGradient>
          </defs>
        </svg>

        <!-- Ambient background -->
        <div class="ambient-bg">
          <div class="ambient-orb ambient-orb--1"></div>
          <div class="ambient-orb ambient-orb--2"></div>
          <div class="ambient-orb ambient-orb--3"></div>
          <div class="ambient-grid"></div>
        </div>

        <!-- Header -->
        <header class="header ${this.navVisible ? '' : 'header--hidden'} ${this.headerScrolled ? 'header--scrolled' : ''}">
          <div class="header-inner">
            <a href="/" class="logo" @click=${(e: Event) => { e.preventDefault(); this.navigate('/'); }}>
              <img src="/logo.svg" alt="TinyIoTHub" onerror="this.style.display='none'" />
              TinyIoTHub
            </a>
            <nav class="nav-links">
              <a href="https://docs.tinyiothub.com" target="_blank">文档</a>
              <a href="https://github.com/Grong/tinyiothub" target="_blank">GitHub</a>
            </nav>
            <div class="header-actions">
              <theme-toggle .theme=${this.theme} @theme-change=${this.handleThemeChange}></theme-toggle>
              ${this.isAuthenticated
                ? html`<button class="btn btn--primary" @click=${() => this.navigate('/dashboard')}>控制台</button>`
                : html`
                  <button class="btn btn--ghost" @click=${() => this.navigate('/login')}>登录</button>
                  <button class="btn btn--primary" @click=${() => this.navigate('/login')}>免费试用</button>
                `}
            </div>
          </div>
        </header>

        <!-- Hero -->
        <section class="hero">
          <div class="hero-grid">
            <div class="hero-content">
              <div class="hero-badge">
                <span class="hero-badge-dot"></span>
                AI 原生 · 边缘智能
              </div>
              <h1>
                设备接入到自愈运维<br />
                <span class="gradient">一句话的事</span>
              </h1>
              <p class="hero-desc">
                轻量级工业边缘 IoT 平台。支持 Modbus/ONVIF/SNMP/MQTT 多协议设备接入，内置 L0-L3 分级自愈引擎，用自然语言即可完成设备配置、状态查询和故障排查。
              </p>
              <div class="hero-cta">
                <button class="btn btn--primary btn--lg" @click=${() => this.navigate('/login')}>
                  开始免费试用
                  <svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><path d="M5 12h14M12 5l7 7-7 7"/></svg>
                </button>
                <a href="https://docs.tinyiothub.com" target="_blank" rel="noopener noreferrer" class="btn btn--ghost btn--lg">
                  查看文档
                </a>
              </div>
              <div class="hero-meta">
                <span class="meta-label">支持协议</span>
                <div class="meta-pills">
                  <span class="meta-pill">Modbus</span>
                  <span class="meta-pill">ONVIF</span>
                  <span class="meta-pill">SNMP</span>
                  <span class="meta-pill">MQTT</span>
                </div>
              </div>
            </div>
          </div>
          <div class="hero-visual">
            <point-earth></point-earth>
          </div>
        </section>

        <!-- Stats -->
        <section class="stats-section reveal">
          <div class="section-inner">
            <div class="stats-header">
              <div class="section-label">平台数据</div>
              <h2 class="stats-title">开源 <span>轻量部署</span></h2>
              <p class="stats-desc">TinyIoTHub 专为工业边缘场景设计，单进程 ~80MB 内存即可运行，覆盖工厂、楼宇、能源、农业等典型 IoT 场景。</p>
            </div>
            <div class="stats-grid">
              <div class="stats-item reveal reveal-delay-1">
                <div class="stats-item__num">4</div>
                <div class="stats-item__label">核心协议支持</div>
              </div>
              <div class="stats-item reveal reveal-delay-2">
                <div class="stats-item__num">~80MB</div>
                <div class="stats-item__label">内存占用</div>
              </div>
              <div class="stats-item reveal reveal-delay-3">
                <div class="stats-item__num">5</div>
                <div class="stats-item__label">种告警条件</div>
              </div>
              <div class="stats-item reveal reveal-delay-1">
                <div class="stats-item__num">L0-L3</div>
                <div class="stats-item__label">全栈自愈等级</div>
              </div>
              <div class="stats-item reveal reveal-delay-2">
                <div class="stats-item__num">MIT</div>
                <div class="stats-item__label">开源协议</div>
              </div>
              <div class="stats-item reveal reveal-delay-3">
                <div class="stats-item__num">MCP</div>
                <div class="stats-item__label">原生 AI 接入</div>
              </div>
            </div>
          </div>
        </section>

        <!-- Features -->
        <section class="section features-section reveal">
          <div class="section-inner">
            <div class="section-header">
              <div class="section-label">核心能力</div>
              <h2 class="section-title">四大核心能力</h2>
              <p class="section-desc">
                从设备接入到智能运维，覆盖工业边缘全生命周期
              </p>
            </div>
            <div class="feature-grid">
              <div class="feature-card reveal reveal-delay-1">
                <div class="feature-icon">
                  <svg viewBox="0 0 24 24" fill="none" stroke="url(#icon-grad)" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round"><rect x="4" y="4" width="16" height="16" rx="2" ry="2"/><rect x="9" y="9" width="6" height="6"/><line x1="9" y1="1" x2="9" y2="4"/><line x1="15" y1="1" x2="15" y2="4"/><line x1="9" y1="20" x2="9" y2="23"/><line x1="15" y1="20" x2="15" y2="23"/><line x1="20" y1="9" x2="23" y2="9"/><line x1="20" y1="14" x2="23" y2="14"/><line x1="1" y1="9" x2="4" y2="9"/><line x1="1" y1="14" x2="4" y2="14"/></svg>
                </div>
                <h3>多协议设备接入</h3>
                <p>Modbus RTU/TCP、ONVIF、SNMP、MQTT，开箱即用</p>
              </div>
              <div class="feature-card reveal reveal-delay-2">
                <div class="feature-icon">
                  <svg viewBox="0 0 24 24" fill="none" stroke="url(#icon-grad)" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round"><path d="M12 3L14.5 8.5L20 11L14.5 13.5L12 19L9.5 13.5L4 11L9.5 8.5L12 3Z"/></svg>
                </div>
                <h3>AI 驱动匹配</h3>
                <p>描述设备类型，自动匹配或生成驱动代码</p>
              </div>
              <div class="feature-card reveal reveal-delay-3">
                <div class="feature-icon">
                  <svg viewBox="0 0 24 24" fill="none" stroke="url(#icon-grad)" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round"><path d="M21 12a9 9 0 0 0-9-9 9.75 9.75 0 0 0-6.74 2.74L3 8"/><path d="M3 3v5h5"/><path d="M3 12a9 9 0 0 0 9 9 9.75 9.75 0 0 0 6.74-2.74L21 16"/><path d="M16 21h5v-5"/></svg>
                </div>
                <h3>L0-L3 自愈引擎</h3>
                <p>system/device/task 三级探针，自动故障检测与恢复</p>
              </div>
              <div class="feature-card reveal reveal-delay-1">
                <div class="feature-icon">
                  <svg viewBox="0 0 24 24" fill="none" stroke="url(#icon-grad)" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round"><path d="M22 12h-4l-3 9L9 3l-3 9H2"/></svg>
                </div>
                <h3>规则引擎</h3>
                <p>阈值、范围、变化、持续时间、组合五种条件，灵活配置告警与自动化</p>
              </div>
              <div class="feature-card reveal reveal-delay-2">
                <div class="feature-icon">
                  <svg viewBox="0 0 24 24" fill="none" stroke="url(#icon-grad)" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round"><path d="M21 11.5a8.38 8.38 0 0 1-.9 3.8 8.5 8.5 0 0 1-7.6 4.7 8.38 8.38 0 0 1-3.8-.9L3 21l1.9-5.7a8.38 8.38 0 0 1-.9-3.8 8.5 8.5 0 0 1 4.7-7.6 8.38 8.38 0 0 1 3.8-.9h.5a8.48 8.48 0 0 1 8 8v.5z"/></svg>
                </div>
                <h3>自然语言运维</h3>
                <p>用日常语言配置设备、查询状态、排查故障，内嵌 MCP Server</p>
              </div>
              <div class="feature-card reveal reveal-delay-3">
                <div class="feature-icon">
                  <svg viewBox="0 0 24 24" fill="none" stroke="url(#icon-grad)" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round"><path d="M20 7l-8-4-8 4m16 0l-8 4m8-4v10l-8 4m0-10L4 7m8 4v10M4 7v10l8 4"/></svg>
                </div>
                <h3>轻量部署</h3>
                <p>~80MB 内存占用，SQLite 零依赖，MIT 开源协议</p>
              </div>
            </div>
          </div>
        </section>

        <!-- Showcase -->
        <section class="section showcase-section reveal">
          <div class="section-inner">
            <div class="showcase-header">
              <div class="section-label">云端协同</div>
              <h2 class="section-title">状态上报 · 工单联动 · 知识闭环</h2>
            </div>
            <div class="showcase-grid">
              <div class="showcase-visual reveal reveal-delay-1">
                <showcase-viz></showcase-viz>
              </div>
              <div class="showcase-content">
                <p class="section-desc" style="text-align: left; margin: 0 0 28px 0;">
                  边缘侧与云端实时协同，设备状态即时同步，故障自动触发工单，构建完整的运维知识库。
                </p>
                <div class="showcase-list">
                  <div class="showcase-item reveal reveal-delay-1">
                    <div class="showcase-item__icon">
                      <svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.5"><path d="M14.7 6.3a1 1 0 0 0 0 1.4l1.6 1.6a1 1 0 0 0 1.4 0l3.77-3.77a6 6 0 0 1-7.94 7.94l-6.91 6.91a2.12 2.12 0 0 1-3-3l6.91-6.91a6 6 0 0 1 7.94-7.94l-3.76 3.76z"/></svg>
                    </div>
                    <div>
                      <div class="showcase-item__title">智慧工厂</div>
                      <div class="showcase-item__desc">老旧设备数字化改造，分钟级接入，零布线施工</div>
                    </div>
                  </div>
                  <div class="showcase-item reveal reveal-delay-2">
                    <div class="showcase-item__icon">
                      <svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.5"><path d="M3 9l9-7 9 7v11a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2z"/><polyline points="9 22 9 12 15 12 15 22"/></svg>
                    </div>
                    <div>
                      <div class="showcase-item__title">智慧楼宇</div>
                      <div class="showcase-item__desc">多系统统一接入，自然语言运维，降低管理成本</div>
                    </div>
                  </div>
                  <div class="showcase-item reveal reveal-delay-3">
                    <div class="showcase-item__icon">
                      <svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.5"><polygon points="13 2 3 14 12 14 11 22 21 10 12 10 13 2"/></svg>
                    </div>
                    <div>
                      <div class="showcase-item__title">分布式能源</div>
                      <div class="showcase-item__desc">边缘自治调度，断网不断服，保障能源系统稳定运行</div>
                    </div>
                  </div>
                  <div class="showcase-item reveal reveal-delay-1">
                    <div class="showcase-item__icon">
                      <svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.5"><path d="M12 22s8-4 8-10V5l-8-3-8 3v7c0 6 8 10 8 10z"/></svg>
                    </div>
                    <div>
                      <div class="showcase-item__title">智慧农业</div>
                      <div class="showcase-item__desc">大棚环境监测、灌溉联动控制，低功耗广域覆盖</div>
                    </div>
                  </div>
                  <div class="showcase-item reveal reveal-delay-2">
                    <div class="showcase-item__icon">
                      <svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.5"><rect x="4" y="4" width="16" height="16" rx="2" ry="2"/><rect x="9" y="9" width="6" height="6"/><line x1="9" y1="1" x2="9" y2="4"/></svg>
                    </div>
                    <div>
                      <div class="showcase-item__title">智慧园区</div>
                      <div class="showcase-item__desc">安防、停车、能耗一体化管理，提升运营效率</div>
                    </div>
                  </div>
                  <div class="showcase-item reveal reveal-delay-3">
                    <div class="showcase-item__icon">
                      <svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.5"><path d="M22 17H2a3 3 0 0 0 3-3V9a7 7 0 0 1 14 0v5a3 3 0 0 0 3 3zm-8.27 4a2 2 0 0 1-3.46 0"/></svg>
                    </div>
                    <div>
                      <div class="showcase-item__title">冷链物流</div>
                      <div class="showcase-item__desc">全程温湿度追踪，异常实时告警，保障货品品质</div>
                    </div>
                  </div>
                </div>
              </div>
            </div>
          </div>
        </section>

        <!-- CTA -->
        <section class="cta-section reveal">
          <div class="cta-inner">
            <h2 class="cta-title">准备好开始了吗？</h2>
            <p class="cta-desc">几分钟接入第一台设备，体验自然语言运维。开源免费，支持私有化部署。</p>
            <div class="cta-buttons">
              <button class="btn btn--primary btn--lg" @click=${() => this.navigate('/login')}>
                免费开始使用
                <svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><path d="M5 12h14M12 5l7 7-7 7"/></svg>
              </button>
              <a href="https://github.com/Grong/tinyiothub" target="_blank" rel="noopener noreferrer" class="btn btn--ghost btn--lg">
                <svg width="18" height="18" fill="currentColor" viewBox="0 0 24 24">
                  <path d="M12 2C6.477 2 2 6.484 2 12.017c0 4.425 2.865 8.18 6.839 9.504.5.092.682-.217.682-.483 0-.237-.008-.868-.013-1.703-2.782.605-3.369-1.343-3.369-1.343-.454-1.158-1.11-1.466-1.11-1.466-.908-.62.069-.608.069-.608 1.003.07 1.531 1.032 1.531 1.032.892 1.53 2.341 1.088 2.91.832.092-.647.35-1.088.636-1.338-2.22-.253-4.555-1.113-4.555-4.951 0-1.093.39-1.988 1.029-2.688-.103-.253-.446-1.272.098-2.65 0 0 .84-.27 2.75 1.026A9.564 9.564 0 0112 6.844c.85.004 1.705.115 2.504.337 1.909-1.296 2.747-1.027 2.747-1.027.546 1.379.202 2.398.1 2.651.64.7 1.028 1.595 1.028 2.688 0 3.848-2.339 4.695-4.566 4.943.359.309.678.92.678 1.855 0 1.338-.012 2.419-.012 2.747 0 .268.18.58.688.482A10.019 10.019 0 0022 12.017C22 6.484 17.522 2 12 2z"/>
                </svg>
                查看 GitHub
              </a>
            </div>
          </div>
        </section>

        <!-- Footer -->
        <footer class="footer">
          <div class="footer-inner">
            <div class="footer-brand">
              <img src="/logo.svg" alt="TinyIoTHub" onerror="this.style.display='none'" />
              <div class="footer-brand-name">TinyIoTHub</div>
            </div>
            <div class="footer-links">
              <a href="https://github.com/Grong/tinyiothub" target="_blank">GitHub</a>
              <a href="https://docs.tinyiothub.com" target="_blank">文档</a>
              <a href="/login" @click=${(e: Event) => { e.preventDefault(); this.navigate('/login'); }}>登录</a>
            </div>
            <p class="footer-copy">&copy; 2026 TinyIoTHub. All rights reserved.</p>
          </div>
        </footer>
      </div>
    `;
  }
}
