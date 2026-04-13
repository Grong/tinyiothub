import { LitElement, html } from "lit";
import { customElement, state } from "lit/decorators.js";
import { getAuthToken } from "../../api/client.js";
import { loadSettings, saveSettings, type UiSettings } from "../storage.js";
import { resolveTheme, type ThemeMode, type ResolvedTheme } from "../theme.js";
import "../components/theme-toggle.js";
import "./home-panel.js";

@customElement("view-home")
export class HomeView extends LitElement {
  @state() isAuthenticated = false;
  @state() navVisible = true;
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
    this.lastScrollY = y;
  };

  createRenderRoot() {
    return this;
  }

  connectedCallback() {
    super.connectedCallback();
    const token = getAuthToken();
    this.isAuthenticated = !!token;
    window.addEventListener("scroll", this.scrollHandler, { passive: true });
    this.loadTheme();
  }

  disconnectedCallback() {
    super.disconnectedCallback();
    window.removeEventListener("scroll", this.scrollHandler);
    if (this.themeMediaQuery) {
      this.themeMediaQuery.removeEventListener("change", this.themeChangeHandler);
    }
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
      <style>
        view-home .home {
          font-family: var(--font-body);
          color: #e8ecf1;
          background: #02040a;
          min-height: 100vh;
          overflow-x: hidden;
        }

        /* Background grid + particles */
        view-home .bg-grid {
          position: fixed;
          inset: 0;
          pointer-events: none;
          z-index: 0;
          background-image:
            radial-gradient(circle at 15% 25%, rgba(0, 212, 255, 0.08) 0%, transparent 45%),
            radial-gradient(circle at 85% 15%, rgba(123, 97, 255, 0.06) 0%, transparent 40%),
            radial-gradient(circle at 70% 80%, rgba(0, 152, 255, 0.05) 0%, transparent 50%),
            linear-gradient(rgba(255,255,255,0.03) 1px, transparent 1px),
            linear-gradient(90deg, rgba(255,255,255,0.03) 1px, transparent 1px);
          background-size: 100% 100%, 100% 100%, 100% 100%, 80px 80px, 80px 80px;
        }

        view-home .bg-grid::after {
          content: '';
          position: absolute;
          inset: 0;
          background: radial-gradient(ellipse at center, transparent 0%, #02040a 85%);
        }

        /* Header */
        view-home .header {
          position: fixed;
          top: 0;
          left: 0;
          right: 0;
          z-index: 100;
          padding: 16px 32px;
          background: transparent;
          transition: transform 0.3s ease, background 0.3s ease;
        }

        view-home .header--scrolled {
          background: rgba(2, 4, 10, 0.7);
          backdrop-filter: blur(12px);
        }

        view-home .header--hidden {
          transform: translateY(-100%);
        }

        view-home .header-inner {
          max-width: 1280px;
          margin: 0 auto;
          display: flex;
          align-items: center;
          justify-content: space-between;
        }

        view-home .logo {
          display: flex;
          align-items: center;
          gap: 10px;
          font-size: 20px;
          font-weight: 700;
          color: #fff;
          text-decoration: none;
        }

        view-home .logo img {
          display: block;
          width: 36px;
          height: 36px;
        }

        view-home .nav-links {
          display: flex;
          align-items: center;
          gap: 40px;
        }

        view-home .nav-links a {
          color: rgba(232, 236, 241, 0.7);
          text-decoration: none;
          font-size: 14px;
          font-weight: 500;
          transition: color 0.2s;
        }

        view-home .nav-links a:hover {
          color: #fff;
        }

        view-home .header-actions {
          display: flex;
          align-items: center;
          gap: 16px;
        }

        /* Buttons */
        view-home .btn {
          display: inline-flex;
          align-items: center;
          justify-content: center;
          gap: 8px;
          padding: 10px 22px;
          border-radius: 8px;
          font-size: 14px;
          font-weight: 600;
          text-decoration: none;
          cursor: pointer;
          border: none;
          transition: all 0.2s ease;
        }

        view-home .btn--ghost {
          background: transparent;
          color: #fff;
          border: 1px solid rgba(255,255,255,0.15);
        }

        view-home .btn--ghost:hover {
          background: rgba(255,255,255,0.06);
          border-color: rgba(255,255,255,0.25);
        }

        view-home .btn--primary {
          background: linear-gradient(135deg, #00d4ff 0%, #0098FF 50%, #7b61ff 100%);
          color: #fff;
          box-shadow: 0 2px 16px rgba(0, 212, 255, 0.35);
        }

        view-home .btn--primary:hover {
          transform: translateY(-1px);
          box-shadow: 0 4px 24px rgba(0, 212, 255, 0.5);
        }

        view-home .btn--lg {
          padding: 14px 28px;
          font-size: 15px;
        }

        /* Hero */
        view-home .hero {
          position: relative;
          z-index: 1;
          padding: 140px 32px 80px;
          max-width: 1280px;
          margin: 0 auto;
        }

        view-home .hero-grid {
          display: grid;
          grid-template-columns: 1.1fr 1fr;
          gap: 60px;
          align-items: center;
        }

        view-home .hero-badge {
          display: inline-flex;
          align-items: center;
          gap: 8px;
          padding: 6px 14px;
          background: rgba(0, 212, 255, 0.08);
          border-radius: 9999px;
          font-size: 13px;
          font-weight: 500;
          color: #00d4ff;
          margin-bottom: 24px;
          border: 1px solid rgba(0, 212, 255, 0.18);
        }

        view-home .hero-badge-dot {
          width: 6px;
          height: 6px;
          border-radius: 50%;
          background: #00d4ff;
          box-shadow: 0 0 8px rgba(0, 212, 255, 0.8);
          animation: pulse-glow 2s ease-in-out infinite;
        }

        view-home .hero h1 {
          font-size: clamp(44px, 5.5vw, 72px);
          font-weight: 800;
          line-height: 1.08;
          color: #fff;
          margin: 0 0 24px;
          letter-spacing: -0.02em;
        }

        view-home .hero h1 .gradient {
          background: linear-gradient(135deg, #00d4ff 0%, #0098FF 45%, #a855f7 100%);
          -webkit-background-clip: text;
          -webkit-text-fill-color: transparent;
          background-clip: text;
        }

        view-home .hero-desc {
          font-size: 17px;
          line-height: 1.75;
          color: rgba(232, 236, 241, 0.65);
          margin: 0 0 36px;
          max-width: 520px;
        }

        view-home .hero-cta {
          display: flex;
          align-items: center;
          gap: 14px;
          flex-wrap: wrap;
        }

        /* Hero visual - 3D cube */
        view-home .hero-visual {
          display: flex;
          align-items: center;
          justify-content: center;
          position: relative;
          height: 420px;
        }

        view-home .cube-scene {
          width: 160px;
          height: 160px;
          perspective: 600px;
        }

        view-home .cube {
          width: 100%;
          height: 100%;
          position: relative;
          transform-style: preserve-3d;
          animation: cube-spin 12s linear infinite;
        }

        view-home .cube-face {
          position: absolute;
          width: 160px;
          height: 160px;
          background: rgba(0, 212, 255, 0.06);
          border: 1px solid rgba(0, 212, 255, 0.25);
          box-shadow: 0 0 30px rgba(0, 212, 255, 0.15) inset;
          display: flex;
          align-items: center;
          justify-content: center;
          font-size: 12px;
          color: rgba(0, 212, 255, 0.8);
          backdrop-filter: blur(4px);
        }

        view-home .cube-face--front  { transform: rotateY(0deg) translateZ(80px); }
        view-home .cube-face--right  { transform: rotateY(90deg) translateZ(80px); }
        view-home .cube-face--back   { transform: rotateY(180deg) translateZ(80px); }
        view-home .cube-face--left   { transform: rotateY(-90deg) translateZ(80px); }
        view-home .cube-face--top    { transform: rotateX(90deg) translateZ(80px); }
        view-home .cube-face--bottom { transform: rotateX(-90deg) translateZ(80px); }

        view-home .cube-ring {
          position: absolute;
          inset: -60px;
          border: 1px solid rgba(0, 212, 255, 0.12);
          border-radius: 50%;
          animation: ring-spin 8s linear infinite;
        }

        view-home .cube-ring--2 {
          inset: -100px;
          border-color: rgba(123, 97, 255, 0.1);
          animation-direction: reverse;
          animation-duration: 14s;
        }

        view-home .cube-glow {
          position: absolute;
          width: 300px;
          height: 300px;
          background: radial-gradient(circle, rgba(0, 212, 255, 0.15) 0%, transparent 60%);
          filter: blur(40px);
          pointer-events: none;
        }

        /* Protocols mini bar */
        view-home .hero-meta {
          margin-top: 48px;
          display: flex;
          align-items: center;
          gap: 24px;
          flex-wrap: wrap;
        }

        view-home .meta-label {
          font-size: 13px;
          color: rgba(232, 236, 241, 0.45);
        }

        view-home .meta-pills {
          display: flex;
          gap: 10px;
          flex-wrap: wrap;
        }

        view-home .meta-pill {
          padding: 6px 12px;
          border-radius: 6px;
          font-size: 12px;
          font-weight: 500;
          color: rgba(232, 236, 241, 0.8);
          background: rgba(255,255,255,0.04);
          border: 1px solid rgba(255,255,255,0.06);
        }

        /* Sections common */
        view-home .section {
          position: relative;
          z-index: 1;
          padding: 90px 32px;
        }

        view-home .section-inner {
          max-width: 1280px;
          margin: 0 auto;
        }

        view-home .section-header {
          text-align: center;
          margin-bottom: 56px;
        }

        view-home .section-label {
          display: inline-flex;
          align-items: center;
          gap: 8px;
          font-size: 13px;
          font-weight: 600;
          color: #00d4ff;
          text-transform: uppercase;
          letter-spacing: 0.12em;
          margin-bottom: 14px;
        }

        view-home .section-label::before {
          content: '';
          width: 18px;
          height: 1px;
          background: linear-gradient(90deg, transparent, #00d4ff);
        }

        view-home .section-label::after {
          content: '';
          width: 18px;
          height: 1px;
          background: linear-gradient(90deg, #00d4ff, transparent);
        }

        view-home .section-title {
          font-size: clamp(30px, 4vw, 44px);
          font-weight: 700;
          color: #fff;
          margin: 0 0 14px;
        }

        view-home .section-desc {
          font-size: 16px;
          color: rgba(232, 236, 241, 0.55);
          max-width: 560px;
          margin: 0 auto;
          line-height: 1.65;
        }

        /* Stats row */
        view-home .stats-row {
          display: grid;
          grid-template-columns: repeat(4, 1fr);
          gap: 24px;
          margin-bottom: 90px;
        }

        view-home .stat-item {
          padding: 28px;
          border-radius: 16px;
          background: rgba(255,255,255,0.02);
          text-align: center;
        }

        view-home .stat-item__value {
          font-size: 42px;
          font-weight: 800;
          background: linear-gradient(135deg, #00d4ff 0%, #0098FF 50%, #a855f7 100%);
          -webkit-background-clip: text;
          -webkit-text-fill-color: transparent;
          background-clip: text;
          line-height: 1;
          margin-bottom: 8px;
        }

        view-home .stat-item__label {
          font-size: 14px;
          color: rgba(232, 236, 241, 0.55);
        }

        /* Feature cards */
        view-home .feature-grid {
          display: grid;
          grid-template-columns: repeat(3, 1fr);
          gap: 20px;
        }

        view-home .feature-card {
          padding: 28px;
          border-radius: 16px;
          background: rgba(255,255,255,0.02);
          transition: transform 0.25s ease, background 0.25s ease;
        }

        view-home .feature-card:hover {
          background: rgba(255,255,255,0.04);
          transform: translateY(-4px);
        }

        view-home .feature-icon {
          width: 40px;
          height: 40px;
          border-radius: 10px;
          display: flex;
          align-items: center;
          justify-content: center;
          font-size: 18px;
          margin-bottom: 18px;
          background: rgba(0, 212, 255, 0.1);
          color: #00d4ff;
          box-shadow: 0 0 20px rgba(0, 212, 255, 0.1);
        }

        view-home .feature-card h3 {
          font-size: 17px;
          font-weight: 600;
          color: #fff;
          margin: 0 0 8px;
        }

        view-home .feature-card p {
          font-size: 14px;
          line-height: 1.65;
          color: rgba(232, 236, 241, 0.5);
          margin: 0;
        }

        /* Large showcase section */
        view-home .showcase {
          padding: 90px 32px;
        }

        view-home .showcase-grid {
          display: grid;
          grid-template-columns: 1fr 1fr;
          gap: 80px;
          align-items: center;
        }

        view-home .showcase-visual {
          position: relative;
          min-height: 360px;
          border-radius: 20px;
          background: rgba(255,255,255,0.02);
          display: flex;
          align-items: center;
          justify-content: center;
          overflow: hidden;
        }

        view-home .showcase-visual::before {
          content: '';
          position: absolute;
          inset: 0;
          background: radial-gradient(circle at 30% 30%, rgba(0, 212, 255, 0.08), transparent 50%);
        }

        view-home .orbit {
          position: absolute;
          width: 200px;
          height: 200px;
          border: 1px dashed rgba(0, 212, 255, 0.2);
          border-radius: 50%;
          animation: orbit-rotate 10s linear infinite;
        }

        view-home .orbit-dot {
          position: absolute;
          top: -6px;
          left: 50%;
          transform: translateX(-50%);
          width: 12px;
          height: 12px;
          border-radius: 50%;
          background: #00d4ff;
          box-shadow: 0 0 16px rgba(0, 212, 255, 0.6);
        }

        view-home .orbit--2 {
          width: 280px;
          height: 280px;
          border-color: rgba(123, 97, 255, 0.15);
          animation-duration: 16s;
          animation-direction: reverse;
        }

        view-home .orbit--2 .orbit-dot {
          background: #a855f7;
          box-shadow: 0 0 16px rgba(168, 85, 247, 0.5);
        }

        view-home .orbit-center {
          width: 80px;
          height: 80px;
          border-radius: 50%;
          background: radial-gradient(circle at 30% 30%, rgba(0, 212, 255, 0.2), rgba(0, 212, 255, 0.05));
          box-shadow: 0 0 40px rgba(0, 212, 255, 0.2);
          display: flex;
          align-items: center;
          justify-content: center;
          font-size: 28px;
          z-index: 1;
        }

        view-home .showcase-list {
          display: flex;
          flex-direction: column;
          gap: 20px;
        }

        view-home .showcase-item {
          display: flex;
          gap: 16px;
          padding: 20px 22px;
          border-radius: 12px;
          background: rgba(255,255,255,0.02);
          transition: background 0.2s;
        }

        view-home .showcase-item:hover {
          background: rgba(255,255,255,0.04);
        }

        view-home .showcase-item__icon {
          width: 36px;
          height: 36px;
          border-radius: 8px;
          display: flex;
          align-items: center;
          justify-content: center;
          flex-shrink: 0;
          background: rgba(0, 212, 255, 0.1);
          color: #00d4ff;
          font-size: 16px;
        }

        view-home .showcase-item__title {
          font-size: 15px;
          font-weight: 600;
          color: #fff;
          margin: 0 0 4px;
        }

        view-home .showcase-item__desc {
          font-size: 13px;
          color: rgba(232, 236, 241, 0.5);
          margin: 0;
          line-height: 1.55;
        }

        /* CTA Section */
        view-home .cta-section {
          padding: 80px 32px 100px;
          position: relative;
          z-index: 1;
        }

        view-home .cta-inner {
          max-width: 900px;
          margin: 0 auto;
          text-align: center;
          padding: 64px 48px;
          border-radius: 24px;
          background: rgba(255,255,255,0.02);
          position: relative;
          overflow: hidden;
        }

        view-home .cta-inner::before {
          content: '';
          position: absolute;
          top: -50%;
          left: -50%;
          width: 200%;
          height: 200%;
          background: radial-gradient(circle, rgba(0, 212, 255, 0.06) 0%, transparent 40%);
          pointer-events: none;
        }

        view-home .cta-title {
          font-size: clamp(30px, 4vw, 44px);
          font-weight: 700;
          color: #fff;
          margin: 0 0 14px;
          position: relative;
        }

        view-home .cta-desc {
          font-size: 16px;
          color: rgba(232, 236, 241, 0.55);
          margin: 0 0 32px;
          position: relative;
        }

        view-home .cta-buttons {
          display: flex;
          align-items: center;
          justify-content: center;
          gap: 14px;
          flex-wrap: wrap;
          position: relative;
        }

        /* Footer */
        view-home .footer {
          padding: 50px 32px 30px;
          border-top: 1px solid rgba(255,255,255,0.04);
          background: #02040a;
          position: relative;
          z-index: 1;
        }

        view-home .footer-inner {
          max-width: 1280px;
          margin: 0 auto;
          display: flex;
          flex-wrap: wrap;
          align-items: center;
          justify-content: space-between;
          gap: 24px;
        }

        view-home .footer-brand {
          display: flex;
          align-items: center;
          gap: 12px;
        }

        view-home .footer-brand img {
          width: 32px;
          height: 32px;
        }

        view-home .footer-brand-name {
          font-size: 16px;
          font-weight: 700;
          color: #fff;
        }

        view-home .footer-links {
          display: flex;
          flex-wrap: wrap;
          gap: 28px;
        }

        view-home .footer-links a {
          font-size: 13px;
          text-decoration: none;
          color: rgba(232, 236, 241, 0.5);
          transition: color 0.2s;
        }

        view-home .footer-links a:hover {
          color: #fff;
        }

        view-home .footer-copy {
          font-size: 13px;
          color: rgba(232, 236, 241, 0.35);
          margin: 0;
        }

        /* Background title */
        view-home .bg-title {
          font-size: clamp(56px, 10vw, 140px);
          font-weight: 900;
          line-height: 1;
          text-align: center;
          color: transparent;
          -webkit-text-stroke: 1px rgba(255,255,255,0.08);
          margin: 0 0 -40px;
          position: relative;
          z-index: 0;
          user-select: none;
        }

        :root[data-theme="light"] view-home .bg-title {
          -webkit-text-stroke: 1px rgba(0,0,0,0.08);
        }

        /* Animations */
        @keyframes cube-spin {
          0% { transform: rotateX(-15deg) rotateY(0deg); }
          100% { transform: rotateX(-15deg) rotateY(360deg); }
        }

        @keyframes ring-spin {
          0% { transform: rotateX(75deg) rotateZ(0deg); }
          100% { transform: rotateX(75deg) rotateZ(360deg); }
        }

        @keyframes orbit-rotate {
          0% { transform: rotate(0deg); }
          100% { transform: rotate(360deg); }
        }

        @keyframes pulse-glow {
          0%, 100% { opacity: 1; box-shadow: 0 0 8px rgba(0, 212, 255, 0.8); }
          50% { opacity: 0.6; box-shadow: 0 0 16px rgba(0, 212, 255, 1); }
        }

        /* Responsive */
        @media (max-width: 1024px) {
          view-home .hero-grid,
          view-home .showcase-grid,
          view-home .big-panel__content {
            grid-template-columns: 1fr;
            gap: 48px;
          }
          view-home .hero-visual {
            order: -1;
            height: 320px;
          }
          view-home .feature-grid {
            grid-template-columns: repeat(2, 1fr);
          }
        }

        @media (max-width: 768px) {
          view-home .nav-links {
            display: none;
          }
          view-home .stats-row {
            grid-template-columns: repeat(2, 1fr);
          }
          view-home .feature-grid {
            grid-template-columns: 1fr;
          }
          view-home .hero {
            padding: 120px 20px 60px;
          }
          view-home .section,
          view-home .showcase {
            padding: 64px 20px;
          }
          view-home .bg-title {
            margin-bottom: -20px;
          }
          view-home .footer-inner {
            flex-direction: column;
            text-align: center;
          }
        }
      </style>

      <div class="home">
        <div class="bg-grid"></div>

        <!-- Header -->
        <header class="header ${this.navVisible ? '' : 'header--hidden'}">
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
                内置人工智能 · 物联行业的 OpenAI
              </div>
              <h1>
                构建下一代<br />
                <span class="gradient">IoT 智能平台</span>
              </h1>
              <p class="hero-desc">
                轻量级、高性能、企业级的物联网边缘网关系统。基于 Rust + AI 构建，为工业物联网场景提供可靠的设备接入、数据采集和边缘计算能力。
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
            <div class="hero-visual">
              <div class="cube-glow"></div>
              <div class="cube-ring"></div>
              <div class="cube-ring cube-ring--2"></div>
              <div class="cube-scene">
                <div class="cube">
                  <div class="cube-face cube-face--front"></div>
                  <div class="cube-face cube-face--back"></div>
                  <div class="cube-face cube-face--right"></div>
                  <div class="cube-face cube-face--left"></div>
                  <div class="cube-face cube-face--top"></div>
                  <div class="cube-face cube-face--bottom"></div>
                </div>
              </div>
            </div>
          </div>
        </section>

        <!-- Big Panel -->
        <section class="section" style="padding-top: 0;">
          <div class="section-inner">
            <div class="bg-title">BUILD IoT</div>
            <home-panel .theme=${this.resolvedTheme}></home-panel>
          </div>
        </section>

        <!-- Features -->
        <section class="section" style="padding-top: 0;">
          <div class="section-inner">
            <div class="section-header">
              <div class="section-label">核心能力</div>
              <h2 class="section-title">边缘智能体驱动的新一代 IoT</h2>
              <p class="section-desc">
                将大模型驱动的智能体嵌入边缘侧，从根本上重塑设备接入与运维流程
              </p>
            </div>
            <div class="feature-grid">
              <div class="feature-card">
                <div class="feature-icon">&#10024;</div>
                <h3>接入即自治</h3>
                <p>自然语言描述设备，自动完成驱动匹配与生成，无需专业背景</p>
              </div>
              <div class="feature-card">
                <div class="feature-icon">&#128260;</div>
                <h3>运行即自愈</h3>
                <p>L0-L3 分级自愈机制，主动发现并修复故障</p>
              </div>
              <div class="feature-card">
                <div class="feature-icon">&#128225;</div>
                <h3>LoRa 无线化</h3>
                <p>免布线施工，内置 LoRa 网关，改造无需停产</p>
              </div>
              <div class="feature-card">
                <div class="feature-icon">&#128187;</div>
                <h3>自然语言交互</h3>
                <p>用日常语言配置设备、查询状态，降低使用门槛</p>
              </div>
              <div class="feature-card">
                <div class="feature-icon">&#129504;</div>
                <h3>智能驱动匹配</h3>
                <p>AI 自动匹配驱动库，无匹配则自动生成并测试验证</p>
              </div>
              <div class="feature-card">
                <div class="feature-icon">&#9889;</div>
                <h3>心跳探针</h3>
                <p>定期自检网关与子设备，提前发现隐患，防患于未然</p>
              </div>
            </div>
          </div>
        </section>

        <!-- Showcase -->
        <section class="showcase">
          <div class="section-inner">
            <div class="showcase-grid">
              <div class="showcase-visual">
                <div class="orbit orbit--2"><div class="orbit-dot"></div></div>
                <div class="orbit"><div class="orbit-dot"></div></div>
                <div class="orbit-center">&#9729;</div>
              </div>
              <div>
                <div class="section-header" style="text-align: left; margin-bottom: 28px;">
                  <div class="section-label" style="justify-content: flex-start;">云端协同</div>
                  <h2 class="section-title" style="margin-bottom: 10px;">状态上报 · 工单联动 · 知识闭环</h2>
                  <p class="section-desc" style="margin: 0; max-width: 420px;">
                    边缘侧与云端实时协同，设备状态即时同步，故障自动触发工单，构建完整的运维知识库。
                  </p>
                </div>
                <div class="showcase-list">
                  <div class="showcase-item">
                    <div class="showcase-item__icon">&#128736;</div>
                    <div>
                      <div class="showcase-item__title">智慧工厂</div>
                      <div class="showcase-item__desc">老旧设备数字化改造，分钟级接入，零布线施工</div>
                    </div>
                  </div>
                  <div class="showcase-item">
                    <div class="showcase-item__icon">&#127969;</div>
                    <div>
                      <div class="showcase-item__title">智慧楼宇</div>
                      <div class="showcase-item__desc">多系统统一接入，自然语言运维，降低管理成本</div>
                    </div>
                  </div>
                  <div class="showcase-item">
                    <div class="showcase-item__icon">&#9889;</div>
                    <div>
                      <div class="showcase-item__title">分布式能源</div>
                      <div class="showcase-item__desc">边缘自治调度，断网不断服，保障能源系统稳定运行</div>
                    </div>
                  </div>
                </div>
              </div>
            </div>
          </div>
        </section>

        <!-- CTA -->
        <section class="cta-section">
          <div class="cta-inner">
            <h2 class="cta-title">准备好开始了吗？</h2>
            <p class="cta-desc">立即部署 TinyIoTHub，开启您的物联网之旅。开源免费，支持私有化部署。</p>
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
