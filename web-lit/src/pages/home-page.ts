import { LitElement, html, css } from 'lit'
import { customElement } from 'lit/decorators.js'
import { navigate } from '../lib/navigate'

@customElement('home-page')
export class HomePage extends LitElement {
  static styles = css`
    :host {
      display: block;
      min-height: 100vh;
      background: var(--bg);
    }

    /* Hero Section */
    .hero {
      position: relative;
      padding: 80px 24px;
      text-align: center;
      background: linear-gradient(180deg, var(--bg-accent) 0%, var(--bg) 100%);
      overflow: hidden;
    }

    .hero-glow {
      position: absolute;
      top: 0;
      left: 50%;
      transform: translateX(-50%);
      width: 600px;
      height: 400px;
      background: radial-gradient(ellipse, var(--accent-subtle) 0%, transparent 70%);
      pointer-events: none;
    }

    .hero-content {
      position: relative;
      z-index: 1;
      max-width: 800px;
      margin: 0 auto;
    }

    .hero-badge {
      display: inline-flex;
      align-items: center;
      gap: 8px;
      padding: 6px 14px;
      background: var(--accent-subtle);
      border: 1px solid var(--accent);
      border-radius: var(--radius-full);
      font-size: 12px;
      color: var(--accent);
      margin-bottom: 24px;
    }

    .hero-title {
      font-size: 48px;
      font-weight: 700;
      color: var(--text-strong);
      margin: 0 0 16px;
      letter-spacing: -0.03em;
      line-height: 1.1;
    }

    .hero-title span {
      background: linear-gradient(135deg, var(--accent) 0%, var(--accent-2) 100%);
      -webkit-background-clip: text;
      -webkit-text-fill-color: transparent;
      background-clip: text;
    }

    .hero-subtitle {
      font-size: 18px;
      color: var(--muted);
      margin: 0 0 32px;
      line-height: 1.6;
      max-width: 600px;
      margin-left: auto;
      margin-right: auto;
    }

    .hero-actions {
      display: flex;
      gap: 12px;
      justify-content: center;
      flex-wrap: wrap;
    }

    .btn-primary {
      display: inline-flex;
      align-items: center;
      gap: 8px;
      padding: 12px 24px;
      background: var(--accent);
      color: var(--accent-foreground);
      border: none;
      border-radius: var(--radius-md);
      font-size: 14px;
      font-weight: 600;
      cursor: pointer;
      text-decoration: none;
      transition: background var(--duration-fast) ease;
    }

    .btn-primary:hover {
      background: var(--accent-hover);
    }

    .btn-secondary {
      display: inline-flex;
      align-items: center;
      gap: 8px;
      padding: 12px 24px;
      background: var(--card);
      color: var(--text);
      border: 1px solid var(--border);
      border-radius: var(--radius-md);
      font-size: 14px;
      font-weight: 500;
      cursor: pointer;
      text-decoration: none;
      transition: border-color var(--duration-fast) ease, background var(--duration-fast) ease;
    }

    .btn-secondary:hover {
      background: var(--bg-hover);
      border-color: var(--border-strong);
    }

    /* Protocols */
    .protocols {
      padding: 32px 24px;
      text-align: center;
      border-bottom: 1px solid var(--border);
    }

    .protocols-label {
      font-size: 12px;
      color: var(--muted);
      margin-bottom: 16px;
    }

    .protocols-list {
      display: flex;
      gap: 12px;
      justify-content: center;
      flex-wrap: wrap;
    }

    .protocol-badge {
      padding: 6px 14px;
      background: var(--card);
      border: 1px solid var(--border);
      border-radius: var(--radius-md);
      font-size: 13px;
      color: var(--text);
    }

    /* Stats Section */
    .stats {
      padding: 48px 24px;
      background: var(--card);
      border-bottom: 1px solid var(--border);
    }

    .stats-grid {
      display: grid;
      grid-template-columns: repeat(4, 1fr);
      gap: 32px;
      max-width: 1000px;
      margin: 0 auto;
    }

    @media (max-width: 768px) {
      .stats-grid {
        grid-template-columns: repeat(2, 1fr);
      }
    }

    .stat-item {
      text-align: center;
    }

    .stat-value {
      font-size: 36px;
      font-weight: 700;
      color: var(--text-strong);
      letter-spacing: -0.03em;
    }

    .stat-label {
      font-size: 14px;
      color: var(--muted);
      margin-top: 4px;
    }

    /* Features Section */
    .features {
      padding: 64px 24px;
    }

    .features-header {
      text-align: center;
      margin-bottom: 48px;
    }

    .features-title {
      font-size: 32px;
      font-weight: 700;
      color: var(--text-strong);
      margin: 0 0 12px;
      letter-spacing: -0.02em;
    }

    .features-subtitle {
      font-size: 16px;
      color: var(--muted);
      margin: 0;
    }

    .features-grid {
      display: grid;
      grid-template-columns: repeat(3, 1fr);
      gap: 24px;
      max-width: 1200px;
      margin: 0 auto;
    }

    @media (max-width: 900px) {
      .features-grid {
        grid-template-columns: repeat(2, 1fr);
      }
    }

    @media (max-width: 600px) {
      .features-grid {
        grid-template-columns: 1fr;
      }
    }

    .feature-card {
      background: var(--card);
      border: 1px solid var(--border);
      border-radius: var(--radius-lg);
      padding: 24px;
      transition: border-color var(--duration-normal) ease, box-shadow var(--duration-normal) ease;
    }

    .feature-card:hover {
      border-color: var(--border-strong);
      box-shadow: var(--shadow-sm);
    }

    .feature-icon {
      width: 40px;
      height: 40px;
      border-radius: var(--radius-md);
      display: flex;
      align-items: center;
      justify-content: center;
      margin-bottom: 16px;
    }

    .feature-icon svg {
      width: 20px;
      height: 20px;
    }

    .feature-icon.blue {
      background: rgba(59, 130, 246, 0.1);
      color: #3b82f6;
    }

    .feature-icon.green {
      background: var(--ok-subtle);
      color: var(--ok);
    }

    .feature-icon.purple {
      background: rgba(139, 92, 246, 0.1);
      color: #8b5cf6;
    }

    .feature-icon.orange {
      background: rgba(249, 115, 22, 0.1);
      color: #f97316;
    }

    .feature-title {
      font-size: 16px;
      font-weight: 600;
      color: var(--text-strong);
      margin: 0 0 8px;
    }

    .feature-desc {
      font-size: 14px;
      color: var(--muted);
      margin: 0;
      line-height: 1.5;
    }

    /* Footer */
    .footer {
      padding: 32px 24px;
      border-top: 1px solid var(--border);
      text-align: center;
    }

    .footer-text {
      font-size: 13px;
      color: var(--muted);
    }

    .footer-text a {
      color: var(--accent);
      text-decoration: none;
    }

    .footer-text a:hover {
      text-decoration: underline;
    }
  `

  render() {
    return html`
      <div class="hero">
        <div class="hero-glow"></div>
        <div class="hero-content">
          <div class="hero-badge">
            <svg width="12" height="12" viewBox="0 0 24 24" fill="currentColor">
              <path d="M12 2L15.09 8.26L22 9.27L17 14.14L18.18 21.02L12 17.77L5.82 21.02L7 14.14L2 9.27L8.91 8.26L12 2Z"/>
            </svg>
            内置人工智能 · 物联行业的 OpenAI
          </div>
          <h1 class="hero-title">
            构建下一代 <span>IoT 平台</span>
          </h1>
          <p class="hero-subtitle">
            轻量级、高性能、企业级的物联网边缘网关系统。基于 Rust + AI 构建，为工业物联网场景提供可靠的设备接入、数据采集和边缘计算能力。
          </p>
          <div class="hero-actions">
            <a href="/signin" class="btn-primary">
              开始免费试用
              <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
                <path stroke-linecap="round" stroke-linejoin="round" d="M13.5 4.5L21 12m0 0l-7.5 7.5M21 12H3"/>
              </svg>
            </a>
            <a href="https://docs.tinyiothub.com" target="_blank" class="btn-secondary">
              查看文档
            </a>
          </div>
        </div>
      </div>

      <div class="protocols">
        <div class="protocols-label">支持的协议</div>
        <div class="protocols-list">
          <span class="protocol-badge">Modbus RTU/TCP</span>
          <span class="protocol-badge">ONVIF 摄像头</span>
          <span class="protocol-badge">SNMP 网络设备</span>
          <span class="protocol-badge">MQTT 消息推送</span>
          <span class="protocol-badge" style="background: var(--accent-subtle); border-color: var(--accent);">9999+ 协议支持</span>
        </div>
      </div>

      <div class="stats">
        <div class="stats-grid">
          <div class="stat-item">
            <div class="stat-value">99.99%</div>
            <div class="stat-label">服务可用性</div>
          </div>
          <div class="stat-item">
            <div class="stat-value">9999+</div>
            <div class="stat-label">协议支持</div>
          </div>
          <div class="stat-item">
            <div class="stat-value">&lt;50ms</div>
            <div class="stat-label">采集延迟</div>
          </div>
          <div class="stat-item">
            <div class="stat-value">7*24</div>
            <div class="stat-label">全天候监控</div>
          </div>
        </div>
      </div>

      <div class="features">
        <div class="features-header">
          <h2 class="features-title">边缘智能体</h2>
          <p class="features-subtitle">AI 驱动的新一代边缘计算平台</p>
        </div>
        <div class="features-grid">
          <div class="feature-card">
            <div class="feature-icon blue">
              <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
                <path stroke-linecap="round" stroke-linejoin="round" d="M9.813 15.904L9 18.75l-.813-2.846a4.5 4.5 0 00-3.09-3.09L2.25 12l2.846-.813a4.5 4.5 0 003.09-3.09L9 5.25l.813 2.846a4.5 4.5 0 003.09 3.09L15.75 12l-2.846.813a4.5 4.5 0 00-3.09 3.09zM18.259 8.715L18 9.75l-.259-1.035a3.375 3.375 0 00-2.455-2.456L14.25 6l1.036-.259a3.375 3.375 0 002.455-2.456L18 2.25l.259 1.035a3.375 3.375 0 002.456 2.456L21.75 6l-1.035.259a3.375 3.375 0 00-2.456 2.456z"/>
              </svg>
            </div>
            <h3 class="feature-title">接入即自治</h3>
            <p class="feature-desc">自然语言描述设备，自动完成驱动匹配与生成</p>
          </div>
          <div class="feature-card">
            <div class="feature-icon green">
              <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
                <path stroke-linecap="round" stroke-linejoin="round" d="M19.5 12c0-1.232-.046-2.453-.138-3.662a4.006 4.006 0 00-3.7-3.7 48.678 48.678 0 00-7.324 0 4.006 4.006 0 00-3.7 3.7c-.017.22-.032.441-.046.662M19.5 12l3-3m-3 3l-3-3m-12 3c0 1.232.046 2.453.138 3.662a4.006 4.006 0 003.7 3.7 48.656 48.656 0 007.324 0 4.006 4.006 0 003.7-3.7c.017-.22.032-.441.046-.662M4.5 12l3 3m-3-3l-3 3"/>
              </svg>
            </div>
            <h3 class="feature-title">运行即自愈</h3>
            <p class="feature-desc">分级自愈机制，主动发现并修复故障</p>
          </div>
          <div class="feature-card">
            <div class="feature-icon purple">
              <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
                <path stroke-linecap="round" stroke-linejoin="round" d="M12 21a9.004 9.004 0 008.716-6.747M12 21a9.004 9.004 0 01-8.716-6.747M12 21c2.485 0 4.5-4.03 4.5-9S14.485 3 12 3m0 18c-2.485 0-4.5-4.03-4.5-9S9.515 3 12 3m0 0a8.997 8.997 0 017.843 4.582M12 3a8.997 8.997 0 00-7.843 4.582m15.686 0A11.953 11.953 0 0112 10.5c-2.998 0-5.74-1.1-7.843-2.918m15.686 0A8.959 8.959 0 0121 12c0 .778-.099 1.533-.284 2.253m0 0A17.919 17.919 0 0112 16.5c-3.162 0-6.133-.815-8.716-2.247m0 0A9.015 9.015 0 013 12c0-1.605.42-3.113 1.157-4.418"/>
              </svg>
            </div>
            <h3 class="feature-title">LoRa无线化</h3>
            <p class="feature-desc">免布线施工，改造无需停产</p>
          </div>
          <div class="feature-card">
            <div class="feature-icon orange">
              <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
                <path stroke-linecap="round" stroke-linejoin="round" d="M3.75 13.5l10.5-11.25L12 10.5h8.25L9.75 21.75 12 13.5H3.75z"/>
              </svg>
            </div>
            <h3 class="feature-title">持续进化</h3>
            <p class="feature-desc">云端驱动库与知识库不断积累</p>
          </div>
        </div>
      </div>

      <div class="footer">
        <p class="footer-text">
          &copy; 2026 TinyIoTHub. All rights reserved.
          <a href="https://beian.miit.gov.cn/" target="_blank">粤ICP备2026029601号-2</a>
        </p>
      </div>
    `
  }
}

declare global {
  interface HTMLElementTagNameMap {
    'home-page': HomePage
  }
}
