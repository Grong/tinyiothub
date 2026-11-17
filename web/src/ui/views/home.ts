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

/**
 * 联系方式 —— 上线前替换为真实信息。
 * 留空的项不会渲染，因此不会出现占位号码被误拨。
 * 二维码图片放到 web/public/wechat-qr.png 即可自动显示。
 */
const CONTACT = {
  wechat: "tinyiothub",
  phone: "",
  email: "hello@tinyiothub.com",
  qr: "/wechat-qr.png",
  hours: "工作日 9:00–18:00",
};

@customElement("view-home")
export class HomeView extends LitElement {
  @state() isAuthenticated = false;
  @state() navVisible = true;
  @state() headerScrolled = false;
  @state() settings: UiSettings = loadSettings();
  @state() theme: ThemeMode = this.settings.theme ?? "system";
  @state() resolvedTheme: ResolvedTheme = "dark";
  @state() private copied = false;
  @state() private qrFailed = false;

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

  private scrollToContact = () => {
    this.querySelector("#contact")?.scrollIntoView({ behavior: "smooth", block: "start" });
  };

  private copyWechat = async () => {
    try {
      await navigator.clipboard.writeText(CONTACT.wechat);
      this.copied = true;
      window.setTimeout(() => {
        this.copied = false;
      }, 2000);
    } catch {
      this.copied = false;
    }
  };

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
                  <button class="btn btn--primary" @click=${this.scrollToContact}>联系我们</button>
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
                IoT Agent Harness · MIT 开源
              </div>
              <h1>
                给设备写 harness<br />
                <span class="gradient">让 AI 真正动手</span>
              </h1>
              <p class="hero-desc">
                Claude Code 给代码写 harness——skills、todo、子代理、审批。TinyIoTHub 把同一套东西写给物理世界：一个常驻 Assistant 被设备事件和定时 tick 唤醒，查本体、做决策、过审批，动手后回读属性验证，全程可审计。
              </p>
              <div class="hero-cta">
                <button class="btn btn--primary btn--lg" @click=${this.scrollToContact}>
                  预约演示
                  <svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><path d="M5 12h14M12 5l7 7-7 7"/></svg>
                </button>
                <button class="btn btn--ghost btn--lg" @click=${() => this.navigate('/login')}>
                  免费试用
                </button>
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

        <!-- Three Layers -->
        <section class="section steps-section reveal">
          <div class="section-inner">
            <div class="section-header">
              <div class="section-label">三层结构 · 价值在中间</div>
              <h2 class="section-title">设备接入是前提，harness 才是产品</h2>
              <p class="section-desc">
                场景差异写进 SKILL.md，不写进循环——新增一个「空压机巡检」不需要改一行 Rust
              </p>
            </div>
            <div class="feature-grid">
              <div class="feature-card reveal reveal-delay-1">
                <div class="step-num">1</div>
                <h3>设备层 · 底座</h3>
                <p>Modbus、ONVIF、SNMP、MQTT 多协议接入，设备、空间、产线统一建模为层级化「物」，属性、事件、操作与知识文档一体定义，支持 DTDL / WoT 导入</p>
              </div>
              <div class="feature-card feature-card--accent reveal reveal-delay-2">
                <div class="step-num">2</div>
                <h3>Harness 层 · 产品本体</h3>
                <p>常驻 Assistant 跑在统一 Session Loop 上：技能按需加载、待办自己维护、执行中可以提问、子代理隔离上下文，每个设备动作都过审批与对账</p>
              </div>
              <div class="feature-card reveal reveal-delay-3">
                <div class="step-num">3</div>
                <h3>应用层 · 产物</h3>
                <p>巡检、故障诊断、心跳监控、告警处置——都是 SKILL.md 工作手册。你写一份新的，大脑就多会一件事，不需要重新编译</p>
              </div>
            </div>
          </div>
        </section>

        <!-- Stats -->
        <section class="stats-section reveal">
          <div class="section-inner">
            <div class="stats-header">
              <div class="section-label">平台数据</div>
              <h2 class="stats-title">开源 <span>轻量部署</span></h2>
              <p class="stats-desc">单进程 ~80MB 即可运行，能塞进任何现场机房和边缘盒子；MIT 开源，随时私有化。</p>
            </div>
            <div class="stats-grid">
              <div class="stats-item reveal reveal-delay-1">
                <div class="stats-item__num">1</div>
                <div class="stats-item__label">常驻 Assistant</div>
              </div>
              <div class="stats-item reveal reveal-delay-2">
                <div class="stats-item__num">~80MB</div>
                <div class="stats-item__label">内存占用</div>
              </div>
              <div class="stats-item reveal reveal-delay-3">
                <div class="stats-item__num">5</div>
                <div class="stats-item__label">harness 核心原语</div>
              </div>
              <div class="stats-item reveal reveal-delay-1">
                <div class="stats-item__num">3</div>
                <div class="stats-item__label">权限模式</div>
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
              <div class="section-label">Harness 部件</div>
              <h2 class="section-title">Harness 的六个部件</h2>
              <p class="section-desc">
                不是功能清单——是一个循环真正跑起来需要的全部零件
              </p>
            </div>
            <div class="feature-grid">
              <div class="feature-card reveal reveal-delay-1">
                <div class="feature-icon">
                  <svg viewBox="0 0 24 24" fill="none" stroke="url(#icon-grad)" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round"><rect x="4" y="4" width="16" height="16" rx="2" ry="2"/><rect x="9" y="9" width="6" height="6"/><line x1="9" y1="1" x2="9" y2="4"/><line x1="15" y1="1" x2="15" y2="4"/><line x1="9" y1="20" x2="9" y2="23"/><line x1="15" y1="20" x2="15" y2="23"/><line x1="20" y1="9" x2="23" y2="9"/><line x1="20" y1="14" x2="23" y2="14"/><line x1="1" y1="9" x2="4" y2="9"/><line x1="1" y1="14" x2="4" y2="14"/></svg>
                </div>
                <h3>统一 Session Loop</h3>
                <p>对话、定时巡检、事件处置共用同一条循环，全平台只有一个执行入口</p>
              </div>
              <div class="feature-card reveal reveal-delay-2">
                <div class="feature-icon">
                  <svg viewBox="0 0 24 24" fill="none" stroke="url(#icon-grad)" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round"><path d="M21 16V8a2 2 0 0 0-1-1.73l-7-4a2 2 0 0 0-2 0l-7 4A2 2 0 0 0 3 8v8a2 2 0 0 0 1 1.73l7 4a2 2 0 0 0 2 0l7-4A2 2 0 0 0 21 16z"/><polyline points="3.27 6.96 12 12.01 20.73 6.96"/><line x1="12" y1="22.08" x2="12" y2="12"/></svg>
                </div>
                <h3>Skill 按需加载</h3>
                <p>SKILL.md 只把名字和描述放进上下文，正文在调用时才注入，不占窗口</p>
              </div>
              <div class="feature-card reveal reveal-delay-3">
                <div class="feature-icon">
                  <svg viewBox="0 0 24 24" fill="none" stroke="url(#icon-grad)" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round"><path d="M21 12a9 9 0 0 0-9-9 9.75 9.75 0 0 0-6.74 2.74L3 8"/><path d="M3 3v5h5"/><path d="M3 12a9 9 0 0 0 9 9 9.75 9.75 0 0 0 6.74-2.74L21 16"/><path d="M16 21h5v-5"/></svg>
                </div>
                <h3>动作可恢复</h3>
                <p>Effect 与 Decision 单赢家，unknown 状态停下对账，绝不自动重放设备动作</p>
              </div>
              <div class="feature-card reveal reveal-delay-1">
                <div class="feature-icon">
                  <svg viewBox="0 0 24 24" fill="none" stroke="url(#icon-grad)" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round"><circle cx="6" cy="6" r="2.5"/><circle cx="18" cy="6" r="2.5"/><circle cx="12" cy="12" r="2.5"/><circle cx="6" cy="18" r="2.5"/><circle cx="18" cy="18" r="2.5"/><line x1="8" y1="7.2" x2="10.3" y2="10.8"/><line x1="16" y1="7.2" x2="13.7" y2="10.8"/><line x1="8" y1="16.8" x2="10.3" y2="13.2"/><line x1="16" y1="16.8" x2="13.7" y2="13.2"/></svg>
                </div>
                <h3>本体驱动决策</h3>
                <p>靠物模型的属性、层级和知识文档判断根因，不需要人把上下文喂进去</p>
              </div>
              <div class="feature-card reveal reveal-delay-2">
                <div class="feature-icon">
                  <svg viewBox="0 0 24 24" fill="none" stroke="url(#icon-grad)" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round"><path d="M21 11.5a8.38 8.38 0 0 1-.9 3.8 8.5 8.5 0 0 1-7.6 4.7 8.38 8.38 0 0 1-3.8-.9L3 21l1.9-5.7a8.38 8.38 0 0 1-.9-3.8 8.5 8.5 0 0 1 4.7-7.6 8.38 8.38 0 0 1 3.8-.9h.5a8.48 8.48 0 0 1 8 8v.5z"/></svg>
                </div>
                <h3>执行中可提问</h3>
                <p>遇到需要你决定的事，给 1–4 个选项阻塞等待；无人在线就暂停任务并通知</p>
              </div>
              <div class="feature-card reveal reveal-delay-3">
                <div class="feature-icon">
                  <svg viewBox="0 0 24 24" fill="none" stroke="url(#icon-grad)" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round"><path d="M22 12h-4l-3 9L9 3l-3 9H2"/></svg>
                </div>
                <h3>待办自维护</h3>
                <p>模型自己维护任务清单，你在时间线上实时看到「计划中 1/3」，不是黑箱</p>
              </div>
            </div>
          </div>
        </section>

        <!-- AI-Native Advantages -->
        <section class="section ai-native-section reveal">
          <div class="section-inner">
            <div class="section-header">
              <div class="section-label">物理世界的约束</div>
              <h2 class="section-title">别的 harness 给不了的三件事</h2>
              <p class="section-desc">
                通用 agent framework 管比特流，永远不会遇到这些约束——这正是它们复制不了的部分
              </p>
            </div>
            <div class="ai-native-grid">
              <div class="ai-native-card reveal reveal-delay-1">
                <div class="ai-native-card__icon">
                  <svg viewBox="0 0 24 24" fill="none" stroke="url(#icon-grad)" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round"><circle cx="5" cy="5" r="2.5"/><circle cx="19" cy="5" r="2.5"/><circle cx="12" cy="12" r="2.5"/><circle cx="5" cy="19" r="2.5"/><circle cx="19" cy="19" r="2.5"/><line x1="7.2" y1="6.2" x2="10.5" y2="10.5"/><line x1="16.8" y1="6.2" x2="13.5" y2="10.5"/><line x1="7.2" y1="17.8" x2="10.5" y2="13.5"/><line x1="16.8" y1="17.8" x2="13.5" y2="13.5"/></svg>
                </div>
                <h3>设备动作不可重试</h3>
                <p>软件里工具调用失败重试是常态；设备上一次「重试」可能是重复关机、重复开阀。所以每个动作都要记「意图 → 派发 → 确认 → 未知」四态。</p>
                <div class="ai-native-card__tags">
                  <span class="ai-native-card__tag">单赢家裁决</span>
                  <span class="ai-native-card__tag">未知状态对账</span>
                  <span class="ai-native-card__tag">幂等恢复</span>
                </div>
              </div>
              <div class="ai-native-card reveal reveal-delay-2">
                <div class="ai-native-card__icon">
                  <svg viewBox="0 0 24 24" fill="none" stroke="url(#icon-grad)" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round"><path d="M21 16V8a2 2 0 0 0-1-1.73l-7-4a2 2 0 0 0-2 0l-7 4A2 2 0 0 0 3 8v8a2 2 0 0 0 1 1.73l7 4a2 2 0 0 0 2 0l7-4A2 2 0 0 0 21 16z"/><polyline points="3.27 6.96 12 12.01 20.73 6.96"/><line x1="12" y1="22.08" x2="12" y2="12"/></svg>
                </div>
                <h3>没有世界模型就没有判断</h3>
                <p>大脑不是被 prompt 唤醒的，是被设备事件和定时 tick 唤醒的。判断依据是物模型的属性、层级和知识文档，不是用户临时贴进去的日志。</p>
                <div class="ai-native-card__tags">
                  <span class="ai-native-card__tag">设备事件唤醒</span>
                  <span class="ai-native-card__tag">本体推理</span>
                  <span class="ai-native-card__tag">知识文档</span>
                </div>
              </div>
              <div class="ai-native-card reveal reveal-delay-3">
                <div class="ai-native-card__icon">
                  <svg viewBox="0 0 24 24" fill="none" stroke="url(#icon-grad)" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round"><path d="M12 2a3 3 0 0 0-3 3v1a4 4 0 0 0-4 4v1.5a2.5 2.5 0 0 0 0 5V18a4 4 0 0 0 4 4h2.5"/><circle cx="19" cy="19" r="3"/><path d="M19 16v2.5M19 22h.01"/></svg>
                </div>
                <h3>动手之后必须验证</h3>
                <p>动作下发不算完成，要回读属性确认真的生效。每一步都进审计账本——谁在什么时候对哪台设备做了什么，都可查。</p>
                <div class="ai-native-card__tags">
                  <span class="ai-native-card__tag">属性回读</span>
                  <span class="ai-native-card__tag">全量审计</span>
                  <span class="ai-native-card__tag">时间线可见</span>
                </div>
              </div>
            </div>
          </div>
        </section>

        <!-- Showcase -->
        <section class="section showcase-section reveal">
          <div class="section-inner">
            <div class="showcase-header">
              <div class="section-label">现场场景</div>
              <h2 class="section-title">一个大脑，六份手册</h2>
            </div>
            <div class="showcase-grid">
              <div class="showcase-visual reveal reveal-delay-1">
                <showcase-viz></showcase-viz>
              </div>
              <div class="showcase-content">
                <p class="section-desc" style="text-align: left; margin: 0 0 28px 0;">
                  巡检、诊断、告警处置都不是平台里的功能菜单，是可读可改的 SKILL.md。换一个现场，加一份手册——不改一行 Rust。
                </p>
                <div class="showcase-list">
                  <div class="showcase-item reveal reveal-delay-1">
                    <div class="showcase-item__icon">
                      <svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.5"><path d="M14.7 6.3a1 1 0 0 0 0 1.4l1.6 1.6a1 1 0 0 0 1.4 0l3.77-3.77a6 6 0 0 1-7.94 7.94l-6.91 6.91a2.12 2.12 0 0 1-3-3l6.91-6.91a6 6 0 0 1 7.94-7.94l-3.76 3.76z"/></svg>
                    </div>
                    <div>
                      <div class="showcase-item__title">工厂 · 夜间巡检</div>
                      <div class="showcase-item__desc">到点自己起来，逐台查离线设备与未处理告警，异常写进待办——早上只给你一份「有问题的清单」</div>
                    </div>
                  </div>
                  <div class="showcase-item reveal reveal-delay-2">
                    <div class="showcase-item__icon">
                      <svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.5"><path d="M3 9l9-7 9 7v11a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2z"/><polyline points="9 22 9 12 15 12 15 22"/></svg>
                    </div>
                    <div>
                      <div class="showcase-item__title">楼宇 · 告警处置</div>
                      <div class="showcase-item__desc">告警进来即唤醒，先沿物模型查关联设备、翻知识文档判断影响范围，再决定动哪个，动手前过审批</div>
                    </div>
                  </div>
                  <div class="showcase-item reveal reveal-delay-3">
                    <div class="showcase-item__icon">
                      <svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.5"><polygon points="13 2 3 14 12 14 11 22 21 10 12 10 13 2"/></svg>
                    </div>
                    <div>
                      <div class="showcase-item__title">能源 · 心跳监控</div>
                      <div class="showcase-item__desc">定期自检网关与子设备，掉线先尝试重连，连续失败才升级到人——不用等巡检时才发现早就离线了</div>
                    </div>
                  </div>
                  <div class="showcase-item reveal reveal-delay-1">
                    <div class="showcase-item__icon">
                      <svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round"><circle cx="11" cy="11" r="7"/><line x1="21" y1="21" x2="16.5" y2="16.5"/></svg>
                    </div>
                    <div>
                      <div class="showcase-item__title">设备厂 · 故障诊断</div>
                      <div class="showcase-item__desc">把属性、历史、驱动日志、上下游关联设备串起来，给出根因假设和验证方法，而不是一句「设备异常」</div>
                    </div>
                  </div>
                  <div class="showcase-item reveal reveal-delay-2">
                    <div class="showcase-item__icon">
                      <svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round"><path d="M21 11.5a8.5 8.5 0 0 1-8.5 8.5 8.4 8.4 0 0 1-3.6-.8L3 21l1.8-5.4A8.5 8.5 0 1 1 21 11.5z"/></svg>
                    </div>
                    <div>
                      <div class="showcase-item__title">多站点 · 随时提问</div>
                      <div class="showcase-item__desc">一句话问「3 号楼 2 层空调怎么一直不制冷」，它自己去查属性、历史、关联设备，不用先学会用界面</div>
                    </div>
                  </div>
                  <div class="showcase-item reveal reveal-delay-3">
                    <div class="showcase-item__icon">
                      <svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round"><path d="M4 4.5A2.5 2.5 0 0 1 6.5 2H20v20H6.5A2.5 2.5 0 0 1 4 19.5z"/><line x1="8" y1="7" x2="16" y2="7"/><line x1="8" y1="11" x2="14" y2="11"/></svg>
                    </div>
                    <div>
                      <div class="showcase-item__title">你的现场 · 写一份手册</div>
                      <div class="showcase-item__desc">把现场规矩写成 markdown 丢进技能目录，下一轮就照着做。不用提需求、不用等排期</div>
                    </div>
                  </div>
                </div>
                <p class="showcase-roadmap">
                  路线图上还有<b>应急预案</b>、<b>节能策略</b>、<b>视频分析</b>——还没到能交付的程度，先标注在这里，不写成现有能力。
                </p>
              </div>
            </div>
          </div>
        </section>

        <!-- Contact -->
        <section class="section contact-section reveal" id="contact">
          <div class="section-inner">
            <div class="contact-card">
              <div class="contact-copy">
                <div class="section-label">联系我们</div>
                <h2 class="contact-title">聊聊你的现场</h2>
                <p class="contact-desc">
                  私有化部署、驱动适配、共创计划——直接找人聊，不用填表。
                </p>
                <div class="contact-list">
                  ${CONTACT.wechat
                    ? html`
                      <button class="contact-item" @click=${this.copyWechat} title="点击复制微信号">
                        <span class="contact-item__icon">
                          <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.6" stroke-linecap="round" stroke-linejoin="round"><path d="M21 11.5a8.38 8.38 0 0 1-.9 3.8 8.5 8.5 0 0 1-7.6 4.7 8.38 8.38 0 0 1-3.8-.9L3 21l1.9-5.7a8.38 8.38 0 0 1-.9-3.8 8.5 8.5 0 0 1 4.7-7.6 8.38 8.38 0 0 1 3.8-.9h.5a8.48 8.48 0 0 1 8 8v.5z"/></svg>
                        </span>
                        <span class="contact-item__body">
                          <span class="contact-item__label">微信</span>
                          <span class="contact-item__value">${CONTACT.wechat}</span>
                        </span>
                        <span class="contact-item__action">${this.copied ? "已复制" : "复制"}</span>
                      </button>
                    `
                    : ""}
                  ${CONTACT.phone
                    ? html`
                      <a class="contact-item" href="tel:${CONTACT.phone}">
                        <span class="contact-item__icon">
                          <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.6" stroke-linecap="round" stroke-linejoin="round"><path d="M22 16.92v3a2 2 0 0 1-2.18 2 19.79 19.79 0 0 1-8.63-3.07 19.5 19.5 0 0 1-6-6 19.79 19.79 0 0 1-3.07-8.67A2 2 0 0 1 4.11 2h3a2 2 0 0 1 2 1.72c.13.96.36 1.9.7 2.81a2 2 0 0 1-.45 2.11L8.09 9.91a16 16 0 0 0 6 6l1.27-1.27a2 2 0 0 1 2.11-.45c.9.34 1.85.57 2.81.7A2 2 0 0 1 22 16.92z"/></svg>
                        </span>
                        <span class="contact-item__body">
                          <span class="contact-item__label">电话</span>
                          <span class="contact-item__value">${CONTACT.phone}</span>
                        </span>
                        <span class="contact-item__action">拨打</span>
                      </a>
                    `
                    : ""}
                  ${CONTACT.email
                    ? html`
                      <a class="contact-item" href="mailto:${CONTACT.email}">
                        <span class="contact-item__icon">
                          <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.6" stroke-linecap="round" stroke-linejoin="round"><rect x="2" y="4" width="20" height="16" rx="2"/><path d="m22 7-8.97 5.7a1.94 1.94 0 0 1-2.06 0L2 7"/></svg>
                        </span>
                        <span class="contact-item__body">
                          <span class="contact-item__label">邮箱</span>
                          <span class="contact-item__value">${CONTACT.email}</span>
                        </span>
                        <span class="contact-item__action">发信</span>
                      </a>
                    `
                    : ""}
                </div>
                <p class="contact-hours">${CONTACT.hours} 回复</p>
              </div>
              <div class="contact-qr">
                ${this.qrFailed
                  ? html`<div class="contact-qr__fallback"><span>二维码待补充</span></div>`
                  : html`<img
                      src=${CONTACT.qr}
                      alt="微信二维码"
                      @error=${() => {
                        this.qrFailed = true;
                      }}
                    />`}
                <div class="contact-qr__caption">扫码加微信</div>
              </div>
            </div>
          </div>
        </section>

        <!-- CTA -->
        <section class="cta-section reveal">
          <div class="cta-inner">
            <h2 class="cta-title">想先自己跑一遍？</h2>
            <p class="cta-desc">几分钟接入第一台设备，Docker 一条命令。开源免费，随时私有化。</p>
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
              <a href="#contact" @click=${(e: Event) => { e.preventDefault(); this.scrollToContact(); }}>联系我们</a>
              ${CONTACT.email ? html`<a href="mailto:${CONTACT.email}">${CONTACT.email}</a>` : ""}
              <a href="/login" @click=${(e: Event) => { e.preventDefault(); this.navigate('/login'); }}>登录</a>
            </div>
            <p class="footer-copy">&copy; 2026 TinyIoTHub. All rights reserved.</p>
          </div>
        </footer>
      </div>
    `;
  }
}
