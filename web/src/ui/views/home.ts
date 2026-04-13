import { LitElement, html } from "lit";
import { customElement, state } from "lit/decorators.js";
import { getAuthToken } from "../../api/client.js";

@customElement("view-home")
export class HomeView extends LitElement {
  @state() isAuthenticated = false;
  @state() navVisible = true;

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
  }

  disconnectedCallback() {
    super.disconnectedCallback();
    window.removeEventListener("scroll", this.scrollHandler);
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
          color: var(--text);
        }

        /* Header */
        view-home .header {
          position: fixed;
          top: 0;
          left: 0;
          right: 0;
          z-index: 100;
          padding: 16px 24px;
          background: var(--chrome);
          backdrop-filter: blur(12px);
          border-bottom: 1px solid var(--border);
          transition: transform 0.3s ease;
        }

        view-home .header--hidden {
          transform: translateY(-100%);
        }

        view-home .header-inner {
          max-width: 1200px;
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
          color: var(--text-strong);
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
          gap: 32px;
        }

        view-home .nav-links a {
          color: var(--muted);
          text-decoration: none;
          font-size: 14px;
          font-weight: 500;
          transition: color 0.2s;
        }

        view-home .nav-links a:hover {
          color: var(--text);
        }

        view-home .header-actions {
          display: flex;
          align-items: center;
          gap: 12px;
        }

        /* Buttons */
        view-home .btn {
          display: inline-flex;
          align-items: center;
          justify-content: center;
          gap: 8px;
          padding: 10px 20px;
          border-radius: 8px;
          font-size: 14px;
          font-weight: 600;
          text-decoration: none;
          cursor: pointer;
          border: none;
          transition: all 0.2s;
          box-shadow: 0 1px 2px rgba(0, 0, 0, 0.05);
        }

        view-home .btn--ghost {
          background: transparent;
          color: var(--text);
          box-shadow: none;
          border: 1px solid var(--border);
        }

        view-home .btn--ghost:hover {
          background: var(--bg-hover);
          box-shadow: 0 1px 2px rgba(0, 0, 0, 0.05);
        }

        :root[data-theme="light"] view-home .btn--ghost {
          background: var(--card);
          color: var(--text-strong);
          border: 1px solid var(--border);
        }

        :root[data-theme="light"] view-home .btn--ghost:hover {
          background: var(--bg-hover);
        }

        view-home .btn--primary {
          background: var(--accent-gradient);
          color: var(--accent-foreground);
          box-shadow: 0 2px 10px var(--accent-glow);
        }

        view-home .btn--primary:hover {
          background: var(--accent-gradient-soft);
          transform: translateY(-2px);
          box-shadow: 0 4px 16px var(--accent-glow-strong);
        }

        :root[data-theme="light"] view-home .btn--primary {
          background: var(--accent-gradient);
          color: var(--accent-foreground);
        }

        :root[data-theme="light"] view-home .btn--primary:hover {
          background: var(--accent-gradient-soft);
        }

        view-home .btn--lg {
          padding: 14px 28px;
          font-size: 16px;
        }

        /* Hero */
        view-home .hero {
          padding: 160px 24px 100px;
          text-align: center;
          position: relative;
          overflow: hidden;
        }

        view-home .hero::before {
          content: '';
          position: absolute;
          top: -200px;
          left: 50%;
          transform: translateX(-50%);
          width: 1000px;
          height: 1000px;
          background: radial-gradient(circle, rgba(0, 212, 255, 0.12) 0%, rgba(123, 97, 255, 0.06) 40%, transparent 70%);
          pointer-events: none;
        }

        :root[data-theme="light"] view-home .hero::before {
          background: radial-gradient(circle, rgba(0, 152, 255, 0.10) 0%, rgba(123, 97, 255, 0.05) 40%, transparent 70%);
        }

        view-home .hero-content {
          max-width: 800px;
          margin: 0 auto;
          position: relative;
          z-index: 1;
        }

        view-home .badge {
          display: inline-flex;
          align-items: center;
          gap: 8px;
          padding: 6px 16px;
          background: rgba(0, 212, 255, 0.08);
          border-radius: 9999px;
          font-size: 13px;
          font-weight: 500;
          color: var(--accent);
          margin-bottom: 24px;
          box-shadow: 0 0 12px var(--accent-glow);
          border: 1px solid rgba(0, 212, 255, 0.25);
        }

        :root[data-theme="light"] view-home .badge {
          border-color: var(--accent);
          background: rgba(0, 152, 255, 0.08);
        }

        view-home .badge-dot {
          display: inline-block;
          width: 8px;
          height: 8px;
          border-radius: 50%;
          background: var(--accent-gradient);
          box-shadow: 0 0 8px var(--accent-glow-strong);
          animation: pulse-glow 2s ease-in-out infinite;
        }

        view-home .hero h1 {
          font-size: clamp(40px, 8vw, 64px);
          font-weight: 800;
          line-height: 1.1;
          color: var(--text-strong);
          margin: 0 0 24px;
          letter-spacing: -0.02em;
        }

        view-home .hero h1 span {
          background: linear-gradient(135deg, var(--accent), var(--accent-hover));
          -webkit-background-clip: text;
          -webkit-text-fill-color: transparent;
          background-clip: text;
        }

        view-home .hero-desc {
          font-size: 18px;
          line-height: 1.7;
          color: var(--muted);
          margin: 0 0 40px;
          max-width: 600px;
          margin-left: auto;
          margin-right: auto;
        }

        view-home .hero-cta {
          display: flex;
          align-items: center;
          justify-content: center;
          gap: 16px;
          flex-wrap: wrap;
        }

        /* Protocol Badges */
        view-home .protocols {
          margin-top: 64px;
        }

        view-home .protocols-label {
          font-size: 14px;
          font-weight: 500;
          color: var(--muted);
          margin: 0 0 16px;
          text-align: center;
        }

        view-home .protocols-grid {
          display: flex;
          flex-wrap: wrap;
          justify-content: center;
          gap: 12px;
        }

        view-home .protocol-badge {
          display: flex;
          align-items: center;
          gap: 6px;
          border-radius: 12px;
          padding: 10px 20px;
          border: 1px solid var(--border);
          background: var(--card);
          box-shadow: 0 1px 3px rgba(0, 0, 0, 0.05);
        }

        view-home .protocol-name {
          font-size: 14px;
          font-weight: 600;
          color: var(--text-strong);
        }

        view-home .protocol-desc {
          font-size: 13px;
          color: var(--muted);
        }

        view-home .protocol-highlight {
          border-radius: 12px;
          padding: 10px 20px;
          background: var(--accent-gradient);
          border: 1px solid rgba(0, 212, 255, 0.35);
          box-shadow: 0 2px 10px var(--accent-glow);
        }

        :root[data-theme="light"] view-home .protocol-highlight {
          background: var(--accent-gradient);
          border-color: rgba(0, 152, 255, 0.35);
        }

        view-home .protocol-highlight-text {
          font-size: 14px;
          font-weight: 600;
          color: var(--accent-foreground);
        }

        /* Stats */
        view-home .stats {
          padding: 80px 24px;
        }

        view-home .stats-inner {
          position: relative;
          max-width: 1200px;
          margin: 0 auto;
          border-radius: 24px;
          padding: 48px;
          background: var(--card);
        }

        view-home .stats-inner::before {
          content: '';
          position: absolute;
          inset: 0;
          border-radius: inherit;
          padding: 1px;
          background: linear-gradient(135deg, rgba(255,255,255,0.12), rgba(255,255,255,0.03));
          -webkit-mask: linear-gradient(#fff 0 0) content-box, linear-gradient(#fff 0 0);
          -webkit-mask-composite: xor;
          mask-composite: exclude;
          pointer-events: none;
        }

        view-home .stats-grid {
          display: grid;
          grid-template-columns: repeat(4, 1fr);
          gap: 32px;
          text-align: center;
        }

        view-home .stat-value {
          font-size: 48px;
          font-weight: 800;
          background: var(--accent-gradient);
          -webkit-background-clip: text;
          -webkit-text-fill-color: transparent;
          background-clip: text;
          line-height: 1;
        }

        view-home .stat-label {
          font-size: 14px;
          color: var(--muted);
          margin-top: 8px;
          font-weight: 500;
        }

        /* Section Common */
        view-home .section-header {
          text-align: center;
          margin-bottom: 60px;
        }

        view-home .section-label {
          display: inline-block;
          font-size: 13px;
          font-weight: 600;
          color: var(--accent);
          text-transform: uppercase;
          letter-spacing: 0.1em;
          margin-bottom: 12px;
        }

        view-home .section-title {
          font-size: clamp(28px, 5vw, 40px);
          font-weight: 700;
          color: var(--text-strong);
          margin: 0 0 16px;
        }

        view-home .section-desc {
          font-size: 16px;
          color: var(--muted);
          max-width: 600px;
          margin: 0 auto;
          line-height: 1.6;
        }

        /* Edge Intelligence Agent */
        view-home .agent-section {
          padding: 96px 24px;
        }

        view-home .agent-inner {
          max-width: 1200px;
          margin: 0 auto;
        }

        view-home .agent-desc-line {
          color: var(--accent);
          font-weight: 600;
        }

        /* Core Features */
        view-home .core-features {
          display: grid;
          grid-template-columns: repeat(4, 1fr);
          gap: 24px;
          margin-bottom: 48px;
        }

        view-home .core-card {
          position: relative;
          border-radius: 16px;
          padding: 28px;
          background: var(--card);
          transition: box-shadow var(--duration-normal) var(--ease-out), transform var(--duration-normal) var(--ease-out);
        }

        view-home .core-card::before {
          content: '';
          position: absolute;
          inset: 0;
          border-radius: inherit;
          padding: 1px;
          background: linear-gradient(135deg, rgba(255,255,255,0.10), rgba(255,255,255,0.03));
          -webkit-mask: linear-gradient(#fff 0 0) content-box, linear-gradient(#fff 0 0);
          -webkit-mask-composite: xor;
          mask-composite: exclude;
          pointer-events: none;
        }

        view-home .core-card:hover {
          box-shadow: 0 8px 32px var(--accent-glow);
          transform: translateY(-4px);
        }

        view-home .core-icon {
          display: inline-flex;
          border-radius: 12px;
          padding: 12px;
          color: #fff;
          margin-bottom: 16px;
          font-size: 20px;
          background: var(--accent-gradient);
          box-shadow: 0 4px 12px var(--accent-glow);
        }

        view-home .core-card h3 {
          font-size: 18px;
          font-weight: 700;
          color: var(--text-strong);
          margin: 0 0 8px;
        }

        view-home .core-card p {
          font-size: 14px;
          line-height: 1.6;
          color: var(--muted);
          margin: 0;
        }

        /* Agent Features */
        view-home .agent-features {
          display: grid;
          grid-template-columns: repeat(3, 1fr);
          gap: 24px;
        }

        view-home .agent-card {
          position: relative;
          border-radius: 16px;
          padding: 32px;
          background: var(--card);
          transition: box-shadow var(--duration-normal) var(--ease-out), transform var(--duration-normal) var(--ease-out);
        }

        view-home .agent-card::before {
          content: '';
          position: absolute;
          inset: 0;
          border-radius: inherit;
          padding: 1px;
          background: linear-gradient(135deg, rgba(255,255,255,0.10), rgba(255,255,255,0.03));
          -webkit-mask: linear-gradient(#fff 0 0) content-box, linear-gradient(#fff 0 0);
          -webkit-mask-composite: xor;
          mask-composite: exclude;
          pointer-events: none;
        }

        view-home .agent-card:hover {
          box-shadow: 0 8px 32px var(--accent-glow);
          transform: translateY(-4px);
        }

        view-home .agent-icon {
          display: inline-flex;
          border-radius: 12px;
          padding: 12px;
          color: #fff;
          margin-bottom: 24px;
          font-size: 20px;
          background: var(--accent-gradient);
          box-shadow: 0 4px 12px var(--accent-glow);
        }

        view-home .agent-card h3 {
          font-size: 20px;
          font-weight: 600;
          color: var(--text-strong);
          margin: 0 0 12px;
        }

        view-home .agent-card p {
          font-size: 15px;
          line-height: 1.6;
          color: var(--muted);
          margin: 0;
        }

        /* Scenarios */
        view-home .scenarios {
          margin-top: 64px;
        }

        view-home .scenarios-title {
          font-size: 20px;
          font-weight: 700;
          text-align: center;
          color: var(--text-strong);
          margin: 0 0 32px;
        }

        view-home .scenarios-grid {
          display: grid;
          grid-template-columns: repeat(3, 1fr);
          gap: 24px;
        }

        view-home .scenario-card {
          border-radius: 16px;
          padding: 28px;
          text-align: center;
          background: var(--accent-subtle);
          border: 1px solid var(--accent-muted);
          transition: all 0.3s;
        }

        :root[data-theme="light"] view-home .scenario-card {
          background: var(--card);
          border: 1px solid var(--border);
        }

        view-home .scenario-card:hover {
          box-shadow: 0 8px 24px rgba(0, 0, 0, 0.1);
          transform: translateY(-4px);
        }

        view-home .scenario-card h4 {
          font-size: 18px;
          font-weight: 600;
          color: var(--text-strong);
          margin: 0 0 8px;
        }

        view-home .scenario-card p {
          font-size: 14px;
          line-height: 1.6;
          color: var(--muted);
          margin: 0;
        }

        /* Agent CTA */
        view-home .agent-cta {
          text-align: center;
          margin-top: 64px;
        }

        /* CTA Section */
        view-home .cta {
          padding: 100px 24px;
          text-align: center;
          position: relative;
        }

        view-home .cta::before {
          content: '';
          position: absolute;
          top: 0;
          left: 25%;
          width: 500px;
          height: 500px;
          background: radial-gradient(circle, var(--accent-subtle), transparent 70%);
          filter: blur(60px);
          pointer-events: none;
        }

        :root[data-theme="light"] view-home .cta::before {
          background: radial-gradient(circle, rgba(220, 38, 38, 0.06), transparent 70%);
        }

        view-home .cta-inner {
          position: relative;
          max-width: 700px;
          margin: 0 auto;
          border-radius: 24px;
          padding: 64px;
          background: var(--card);
          position: relative;
          z-index: 1;
        }

        view-home .cta-inner::before {
          content: '';
          position: absolute;
          inset: 0;
          border-radius: inherit;
          padding: 1px;
          background: linear-gradient(135deg, rgba(255,255,255,0.12), rgba(255,255,255,0.03));
          -webkit-mask: linear-gradient(#fff 0 0) content-box, linear-gradient(#fff 0 0);
          -webkit-mask-composite: xor;
          mask-composite: exclude;
          pointer-events: none;
        }

        view-home .cta h2 {
          font-size: clamp(32px, 6vw, 48px);
          font-weight: 800;
          color: var(--text-strong);
          margin: 0 0 20px;
          line-height: 1.2;
        }

        view-home .cta p {
          font-size: 18px;
          color: var(--muted);
          margin: 0 0 40px;
          line-height: 1.6;
        }

        view-home .cta-buttons {
          display: flex;
          align-items: center;
          justify-content: center;
          gap: 16px;
          flex-wrap: wrap;
        }

        /* Footer */
        view-home .footer {
          padding: 60px 24px 30px;
          border-top: 1px solid var(--border);
          background: var(--bg-elevated);
        }

        view-home .footer-inner {
          max-width: 1200px;
          margin: 0 auto;
          display: flex;
          flex-wrap: wrap;
          align-items: center;
          justify-content: space-between;
          gap: 32px;
        }

        view-home .footer-brand {
          display: flex;
          align-items: center;
          gap: 12px;
        }

        view-home .footer-brand img {
          width: 40px;
          height: 40px;
        }

        view-home .footer-brand-name {
          font-size: 18px;
          font-weight: 700;
          color: var(--text-strong);
        }

        view-home .footer-brand-desc {
          font-size: 14px;
          color: var(--muted);
          margin: 2px 0 0;
        }

        view-home .footer-links {
          display: flex;
          flex-wrap: wrap;
          gap: 24px;
        }

        view-home .footer-links a {
          font-size: 14px;
          text-decoration: none;
          color: var(--muted);
          transition: color 0.2s;
        }

        view-home .footer-links a:hover {
          color: var(--text);
        }

        view-home .footer-copyright {
          font-size: 14px;
          color: var(--muted);
          margin: 0;
        }

        /* Animations */
        @keyframes home-pulse {
          0%, 100% { opacity: 1; }
          50% { opacity: 0.4; }
        }

        /* Responsive */
        @media (max-width: 768px) {
          view-home .nav-links {
            display: none;
          }

          view-home .hero {
            padding: 120px 20px 60px;
          }

          view-home .stats-grid {
            grid-template-columns: repeat(2, 1fr);
          }

          view-home .core-features {
            grid-template-columns: repeat(2, 1fr);
          }

          view-home .agent-features {
            grid-template-columns: 1fr;
          }

          view-home .scenarios-grid {
            grid-template-columns: 1fr;
          }

          view-home .footer-inner {
            flex-direction: column;
            text-align: center;
          }
        }
      </style>

      <div class="home">
        <!-- Header -->
        <header class="header ${this.navVisible ? "" : "header--hidden"}">
          <div class="header-inner">
            <a href="/" class="logo" @click=${(e: Event) => { e.preventDefault(); this.navigate("/"); }}>
              <img src="/logo.svg" alt="TinyIoTHub" onerror="this.style.display='none'" />
              TinyIoTHub
            </a>
            <nav class="nav-links">
              <a href="https://docs.tinyiothub.com" target="_blank">文档</a>
            </nav>
            <div class="header-actions">
              <a href="https://github.com/Grong/tinyiothub" target="_blank" rel="noopener noreferrer" style="color: var(--muted); display: flex; align-items: center;">
                <svg width="20" height="20" fill="currentColor" viewBox="0 0 24 24">
                  <path d="M12 2C6.477 2 2 6.484 2 12.017c0 4.425 2.865 8.18 6.839 9.504.5.092.682-.217.682-.483 0-.237-.008-.868-.013-1.703-2.782.605-3.369-1.343-3.369-1.343-.454-1.158-1.11-1.466-1.11-1.466-.908-.62.069-.608.069-.608 1.003.07 1.531 1.032 1.531 1.032.892 1.53 2.341 1.088 2.91.832.092-.647.35-1.088.636-1.338-2.22-.253-4.555-1.113-4.555-4.951 0-1.093.39-1.988 1.029-2.688-.103-.253-.446-1.272.098-2.65 0 0 .84-.27 2.75 1.026A9.564 9.564 0 0112 6.844c.85.004 1.705.115 2.504.337 1.909-1.296 2.747-1.027 2.747-1.027.546 1.379.202 2.398.1 2.651.64.7 1.028 1.595 1.028 2.688 0 3.848-2.339 4.695-4.566 4.943.359.309.678.92.678 1.855 0 1.338-.012 2.419-.012 2.747 0 .268.18.58.688.482A10.019 10.019 0 0022 12.017C22 6.484 17.522 2 12 2z"/>
                </svg>
              </a>
              ${this.isAuthenticated
                ? html`<a href="/dashboard" class="btn btn--primary" @click=${(e: Event) => { e.preventDefault(); this.navigate("/dashboard"); }}>控制台</a>`
                : html`
                  <button class="btn btn--ghost" @click=${() => this.navigate("/login")}>登录</button>
                  <button class="btn btn--primary" @click=${() => this.navigate("/login")}>免费试用</button>
                `}
            </div>
          </div>
        </header>

        <!-- Hero -->
        <section class="hero">
          <div class="hero-content">
            <div class="badge">
              <span class="badge-dot"></span>
              内置人工智能 · 物联行业的 OpenAI
            </div>
            <h1>
              <span>构建下一代 IoT 平台</span>
            </h1>
            <p class="hero-desc">
              轻量级、高性能、企业级的物联网边缘网关系统。基于 Rust + AI 构建，为工业物联网场景提供可靠的设备接入、数据采集和边缘计算能力。
            </p>
            <div class="hero-cta">
              <button class="btn btn--primary btn--lg" @click=${() => this.navigate("/login")}>
                开始免费试用
                <svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><path d="M5 12h14M12 5l7 7-7 7"/></svg>
              </button>
              <a href="https://docs.tinyiothub.com" target="_blank" rel="noopener noreferrer" class="btn btn--ghost btn--lg">
                查看文档
              </a>
            </div>

            <!-- Protocol Badges -->
            <div class="protocols">
              <p class="protocols-label">支持的协议</p>
              <div class="protocols-grid">
                <div class="protocol-badge">
                  <span class="protocol-name">Modbus</span>
                  <span class="protocol-desc">RTU/TCP</span>
                </div>
                <div class="protocol-badge">
                  <span class="protocol-name">ONVIF</span>
                  <span class="protocol-desc">摄像头</span>
                </div>
                <div class="protocol-badge">
                  <span class="protocol-name">SNMP</span>
                  <span class="protocol-desc">网络设备</span>
                </div>
                <div class="protocol-badge">
                  <span class="protocol-name">MQTT</span>
                  <span class="protocol-desc">消息推送</span>
                </div>
                <div class="protocol-highlight">
                  <span class="protocol-highlight-text">9999+ 协议支持</span>
                </div>
              </div>
            </div>
          </div>
        </section>

        <!-- Stats -->
        <section class="stats">
          <div class="stats-inner">
            <div class="stats-grid">
              <div>
                <div class="stat-value">99.99%</div>
                <div class="stat-label">服务可用性</div>
              </div>
              <div>
                <div class="stat-value">9999+</div>
                <div class="stat-label">协议支持</div>
              </div>
              <div>
                <div class="stat-value">&lt;50ms</div>
                <div class="stat-label">采集延迟</div>
              </div>
              <div>
                <div class="stat-value">7*24</div>
                <div class="stat-label">全天候监控</div>
              </div>
            </div>
          </div>
        </section>

        <!-- Edge Intelligence Agent -->
        <section class="agent-section">
          <div class="agent-inner">
            <div class="section-header">
              <span class="section-label">AI 驱动的新一代边缘计算</span>
              <h2 class="section-title">边缘智能体</h2>
              <p class="section-desc">
                <strong class="agent-desc-line">接入即自治，运行即自愈</strong><br />
                AI 原生自主型边缘计算平台，将大模型驱动的智能体嵌入边缘侧，从根本上重塑设备接入与运维流程
              </p>
            </div>

            <!-- Core Features -->
            <div class="core-features">
              <div class="core-card">
                <div class="core-icon">&#10024;</div>
                <h3>接入即自治</h3>
                <p>自然语言描述设备，自动完成驱动匹配与生成</p>
              </div>
              <div class="core-card">
                <div class="core-icon">&#128260;</div>
                <h3>运行即自愈</h3>
                <p>分级自愈机制，主动发现并修复故障</p>
              </div>
              <div class="core-card">
                <div class="core-icon">&#128225;</div>
                <h3>LoRa无线化</h3>
                <p>免布线施工，改造无需停产</p>
              </div>
              <div class="core-card">
                <div class="core-icon">&#127760;</div>
                <h3>持续进化</h3>
                <p>云端驱动库与知识库不断积累</p>
              </div>
            </div>

            <!-- Agent Features -->
            <div class="agent-features">
              <div class="agent-card">
                <div class="agent-icon">&#128187;</div>
                <h3>自然语言交互</h3>
                <p>用日常语言配置设备、查询状态，无需专业背景</p>
              </div>
              <div class="agent-card">
                <div class="agent-icon">&#129504;</div>
                <h3>智能驱动匹配</h3>
                <p>AI自动匹配驱动库，无匹配则自动生成并测试验证</p>
              </div>
              <div class="agent-card">
                <div class="agent-icon">&#128737;</div>
                <h3>分级自愈机制</h3>
                <p>L0-L3分级处理，从被动响应到主动运维</p>
              </div>
              <div class="agent-card">
                <div class="agent-icon">&#9889;</div>
                <h3>心跳探针</h3>
                <p>定期自检网关与子设备，提前发现隐患</p>
              </div>
              <div class="agent-card">
                <div class="agent-icon">&#9729;</div>
                <h3>云端协同</h3>
                <p>状态上报、工单联动、知识闭环</p>
              </div>
              <div class="agent-card">
                <div class="agent-icon">&#128241;</div>
                <h3>LoRa无线接入</h3>
                <p>内置LoRa网关，远距离低功耗免布线</p>
              </div>
            </div>

            <!-- Scenarios -->
            <div class="scenarios">
              <h3 class="scenarios-title">典型应用场景</h3>
              <div class="scenarios-grid">
                <div class="agent-card">
                  <h4>智慧工厂</h4>
                  <p>老旧设备数字化改造，分钟级接入，零布线</p>
                </div>
                <div class="agent-card">
                  <h4>智慧楼宇</h4>
                  <p>多系统统一接入，自然语言运维</p>
                </div>
                <div class="agent-card">
                  <h4>分布式能源</h4>
                  <p>边缘自治调度，断网不断服</p>
                </div>
              </div>
            </div>

            <!-- Agent CTA -->
            <div class="agent-cta">
              <a href="https://docs.tinyiothub.com" target="_blank" rel="noopener noreferrer"
                class="btn btn--primary btn--lg">
                了解更多
                <svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><path d="M5 12h14M12 5l7 7-7 7"/></svg>
              </a>
            </div>
          </div>
        </section>

        <!-- Final CTA -->
        <section class="cta">
          <div class="cta-inner">
            <h2>准备好开始了吗？</h2>
            <p>立即部署 TinyIoTHub，开启您的物联网之旅。开源免费，支持私有化部署。</p>
            <div class="cta-buttons">
              <button class="btn btn--primary btn--lg" @click=${() => this.navigate("/login")}>
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
              <div>
                <div class="footer-brand-name">TinyIoTHub</div>
                <p class="footer-brand-desc">开源物联网平台</p>
              </div>
            </div>
            <div class="footer-links">
              <a href="https://github.com/Grong/tinyiothub" target="_blank">GitHub</a>
              <a href="https://docs.tinyiothub.com" target="_blank">文档</a>
              <a href="/login" @click=${(e: Event) => { e.preventDefault(); this.navigate("/login"); }}>登录</a>
            </div>
            <p class="footer-copyright">&copy; 2026 TinyIoTHub. All rights reserved.</p>
          </div>
        </footer>
      </div>
    `;
  }
}
