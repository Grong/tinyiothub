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
                AIoT 智能运维平台<br />
                <span class="gradient">一句话的事</span>
              </h1>
              <p class="hero-desc">
                AI 原生 AIoT 平台。融合知识图谱推理、3D 数字孪生与大模型智能运维，支持 Modbus/ONVIF/SNMP/MQTT 多协议接入，内置 L0-L3 自愈引擎。用自然语言完成设备配置、故障排查与数据洞察，内嵌 MCP Server——Claude、Cursor 即连即用。
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
              <p class="stats-desc">TinyIoTHub 专为 AIoT 场景设计，单进程 ~80MB 内存即可运行，覆盖工厂、楼宇、能源、农业等典型 IoT 场景。</p>
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
              <h2 class="section-title">六大核心能力</h2>
              <p class="section-desc">
                从设备接入到 AI 智能运维，覆盖 AIoT 全生命周期
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
                  <svg viewBox="0 0 24 24" fill="none" stroke="url(#icon-grad)" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round"><path d="M21 16V8a2 2 0 0 0-1-1.73l-7-4a2 2 0 0 0-2 0l-7 4A2 2 0 0 0 3 8v8a2 2 0 0 0 1 1.73l7 4a2 2 0 0 0 2 0l7-4A2 2 0 0 0 21 16z"/><polyline points="3.27 6.96 12 12.01 20.73 6.96"/><line x1="12" y1="22.08" x2="12" y2="12"/></svg>
                </div>
                <h3>沉浸式工作空间</h3>
                <p>3D 数字孪生场景 + AI 数据洞察，可折叠执行过程，自然语言交互</p>
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
                  <svg viewBox="0 0 24 24" fill="none" stroke="url(#icon-grad)" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round"><circle cx="6" cy="6" r="2.5"/><circle cx="18" cy="6" r="2.5"/><circle cx="12" cy="12" r="2.5"/><circle cx="6" cy="18" r="2.5"/><circle cx="18" cy="18" r="2.5"/><line x1="8" y1="7.2" x2="10.3" y2="10.8"/><line x1="16" y1="7.2" x2="13.7" y2="10.8"/><line x1="8" y1="16.8" x2="10.3" y2="13.2"/><line x1="16" y1="16.8" x2="13.7" y2="13.2"/></svg>
                </div>
                <h3>知识图谱</h3>
                <p>设备关系拓扑建模，故障影响范围推理，实体与关系灵活定义</p>
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
                  <svg viewBox="0 0 24 24" fill="none" stroke="url(#icon-grad)" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round"><path d="M22 12h-4l-3 9L9 3l-3 9H2"/></svg>
                </div>
                <h3>规则引擎</h3>
                <p>阈值、范围、变化、持续时间、组合五种条件类型，灵活配置告警与自动化规则</p>
              </div>
            </div>
          </div>
        </section>

        <!-- AI-Native Advantages -->
        <section class="section ai-native-section reveal">
          <div class="section-inner">
            <div class="section-header">
              <div class="section-label">AI 原生优势</div>
              <h2 class="section-title">大模型驱动的智能运维</h2>
              <p class="section-desc">
                深度融合 AI 能力，从设备管理到故障排查，用对话替代复杂操作
              </p>
            </div>
            <div class="ai-native-grid">
              <div class="ai-native-card reveal reveal-delay-1">
                <div class="ai-native-card__icon">
                  <svg viewBox="0 0 24 24" fill="none" stroke="url(#icon-grad)" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round"><circle cx="5" cy="5" r="2.5"/><circle cx="19" cy="5" r="2.5"/><circle cx="12" cy="12" r="2.5"/><circle cx="5" cy="19" r="2.5"/><circle cx="19" cy="19" r="2.5"/><line x1="7.2" y1="6.2" x2="10.5" y2="10.5"/><line x1="16.8" y1="6.2" x2="13.5" y2="10.5"/><line x1="7.2" y1="17.8" x2="10.5" y2="13.5"/><line x1="16.8" y1="17.8" x2="13.5" y2="13.5"/></svg>
                </div>
                <h3>知识图谱 · 智能推理</h3>
                <p>自动构建设备关系拓扑，支持实体、属性、关系的灵活建模。故障发生时沿知识图谱推理影响范围，快速定位根因。</p>
                <div class="ai-native-card__tags">
                  <span class="ai-native-card__tag">实体关系建模</span>
                  <span class="ai-native-card__tag">故障影响推理</span>
                  <span class="ai-native-card__tag">可视化图谱</span>
                </div>
              </div>
              <div class="ai-native-card reveal reveal-delay-2">
                <div class="ai-native-card__icon">
                  <svg viewBox="0 0 24 24" fill="none" stroke="url(#icon-grad)" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round"><path d="M21 16V8a2 2 0 0 0-1-1.73l-7-4a2 2 0 0 0-2 0l-7 4A2 2 0 0 0 3 8v8a2 2 0 0 0 1 1.73l7 4a2 2 0 0 0 2 0l7-4A2 2 0 0 0 21 16z"/><polyline points="3.27 6.96 12 12.01 20.73 6.96"/><line x1="12" y1="22.08" x2="12" y2="12"/></svg>
                </div>
                <h3>沉浸式工作空间 · A2UI 驱动</h3>
                <p>3D 数字孪生场景 + 27 种 A2UI 组件，可折叠思考过程与工具执行。自然语言描述需求，AI 自动生成 UI 并执行操作。</p>
                <div class="ai-native-card__tags">
                  <span class="ai-native-card__tag">3D 数字孪生</span>
                  <span class="ai-native-card__tag">A2UI 协议</span>
                  <span class="ai-native-card__tag">过程透明可见</span>
                </div>
              </div>
              <div class="ai-native-card reveal reveal-delay-3">
                <div class="ai-native-card__icon">
                  <svg viewBox="0 0 24 24" fill="none" stroke="url(#icon-grad)" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round"><path d="M12 2a3 3 0 0 0-3 3v1a4 4 0 0 0-4 4v1.5a2.5 2.5 0 0 0 0 5V18a4 4 0 0 0 4 4h2.5"/><circle cx="19" cy="19" r="3"/><path d="M19 16v2.5M19 22h.01"/></svg>
                </div>
                <h3>MCP 原生 · 开放 AI 生态</h3>
                <p>内嵌 MCP Server，Claude Desktop、Cursor 等 AI 工具可直接连接。Agent 自进化记忆系统，运维经验持续积累，越用越智能。</p>
                <div class="ai-native-card__tags">
                  <span class="ai-native-card__tag">MCP 协议</span>
                  <span class="ai-native-card__tag">Claude 集成</span>
                  <span class="ai-native-card__tag">自进化记忆</span>
                </div>
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
