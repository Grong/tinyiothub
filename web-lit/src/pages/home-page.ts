import { LitElement, html, css } from 'lit'
import { customElement, state } from 'lit/decorators.js'
import { navigate } from '../lib/navigate'

@customElement('home-page')
export class HomePage extends LitElement {
  static styles = css`
    :host {
      display: block;
      min-height: 100vh;
      background: var(--bg);
    }

    /* Navigation */
    .nav {
      position: fixed;
      top: 0;
      left: 0;
      right: 0;
      z-index: 50;
      background: var(--chrome);
      border-bottom: 1px solid var(--border);
      backdrop-filter: blur(12px);
    }

    .nav-inner {
      max-width: 1280px;
      margin: 0 auto;
      padding: 0 24px;
      height: 64px;
      display: flex;
      align-items: center;
      justify-content: space-between;
    }

    .nav-left {
      display: flex;
      align-items: center;
      gap: 32px;
    }

    .nav-logo {
      display: flex;
      align-items: center;
      gap: 8px;
      text-decoration: none;
      cursor: pointer;
    }

    .nav-logo svg {
      width: 36px;
      height: 36px;
      color: var(--accent);
    }

    .nav-logo-text {
      font-size: 20px;
      font-weight: 700;
      color: var(--text-strong);
      letter-spacing: -0.02em;
    }

    .nav-links {
      display: flex;
      gap: 4px;
    }

    .nav-link {
      display: flex;
      align-items: center;
      height: 32px;
      padding: 0 12px;
      border-radius: var(--radius-md);
      font-size: 14px;
      font-weight: 500;
      color: var(--text);
      text-decoration: none;
      cursor: pointer;
      transition: background 0.15s ease, color 0.15s ease;
    }

    .nav-link:hover {
      background: var(--bg-hover);
      color: var(--text-strong);
    }

    .nav-right {
      display: flex;
      align-items: center;
      gap: 16px;
    }

    .nav-github {
      color: var(--muted);
      transition: color 0.15s ease;
      cursor: pointer;
    }

    .nav-github:hover {
      color: var(--text-strong);
    }

    .nav-btn {
      height: 32px;
      padding: 0 16px;
      border-radius: var(--radius-md);
      font-size: 14px;
      font-weight: 500;
      cursor: pointer;
      transition: all 0.15s ease;
      border: none;
    }

    .nav-btn.ghost {
      background: transparent;
      color: var(--text);
    }

    .nav-btn.ghost:hover {
      background: var(--bg-hover);
      color: var(--text-strong);
    }

    .nav-btn.primary {
      background: var(--accent);
      color: var(--accent-foreground);
    }

    .nav-btn.primary:hover {
      background: var(--accent-hover);
    }

    /* Hero Section */
    .hero {
      padding: 160px 24px 80px;
      text-align: center;
      background: linear-gradient(180deg, var(--bg-accent) 0%, var(--bg) 100%);
    }

    .hero-badge {
      display: inline-flex;
      align-items: center;
      gap: 8px;
      padding: 6px 12px;
      background: var(--accent-subtle);
      border: 1px solid var(--accent-muted);
      border-radius: var(--radius-full);
      font-size: 12px;
      font-weight: 500;
      color: var(--accent);
      margin-bottom: 24px;
    }

    .hero-title {
      font-size: 64px;
      font-weight: 700;
      line-height: 1.1;
      letter-spacing: -0.03em;
      margin: 0 0 24px;
      max-width: 900px;
      margin-left: auto;
      margin-right: auto;
    }

    .hero-title .gradient {
      background: linear-gradient(135deg, var(--accent) 0%, #ff8a8a 50%, var(--accent-2) 100%);
      -webkit-background-clip: text;
      -webkit-text-fill-color: transparent;
      background-clip: text;
    }

    .hero-desc {
      font-size: 18px;
      line-height: 1.6;
      color: var(--muted);
      max-width: 600px;
      margin: 0 auto 40px;
    }

    .hero-btns {
      display: flex;
      gap: 16px;
      justify-content: center;
      flex-wrap: wrap;
    }

    .btn {
      height: 44px;
      padding: 0 24px;
      border-radius: var(--radius-md);
      font-size: 15px;
      font-weight: 600;
      cursor: pointer;
      transition: all 0.15s ease;
      border: none;
      text-decoration: none;
      display: inline-flex;
      align-items: center;
      gap: 8px;
    }

    .btn.primary {
      background: var(--accent);
      color: var(--accent-foreground);
    }

    .btn.primary:hover {
      background: var(--accent-hover);
      transform: translateY(-1px);
    }

    .btn.secondary {
      background: var(--card);
      color: var(--text-strong);
      border: 1px solid var(--border);
    }

    .btn.secondary:hover {
      background: var(--bg-hover);
      border-color: var(--border-hover);
    }

    /* Protocols */
    .protocols {
      padding: 0 24px 80px;
      background: var(--bg);
    }

    .protocols-inner {
      max-width: 900px;
      margin: 0 auto;
      display: flex;
      flex-wrap: wrap;
      justify-content: center;
      gap: 12px;
    }

    .protocol-badge {
      display: flex;
      align-items: center;
      gap: 8px;
      padding: 8px 16px;
      background: var(--card);
      border: 1px solid var(--border);
      border-radius: var(--radius-md);
      font-size: 14px;
      font-weight: 500;
      color: var(--text);
    }

    .protocol-badge svg {
      width: 18px;
      height: 18px;
      color: var(--accent);
    }

    /* Stats */
    .stats {
      padding: 60px 24px;
      background: var(--bg-accent);
      border-top: 1px solid var(--border);
      border-bottom: 1px solid var(--border);
    }

    .stats-inner {
      max-width: 1000px;
      margin: 0 auto;
      display: grid;
      grid-template-columns: repeat(4, 1fr);
      gap: 32px;
    }

    @media (max-width: 768px) {
      .stats-inner {
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
      letter-spacing: -0.02em;
      margin-bottom: 8px;
    }

    .stat-label {
      font-size: 14px;
      color: var(--muted);
    }

    /* Features */
    .features {
      padding: 100px 24px;
      background: var(--bg);
    }

    .section-header {
      text-align: center;
      margin-bottom: 60px;
    }

    .section-tag {
      display: inline-block;
      padding: 6px 12px;
      background: var(--accent-subtle);
      border-radius: var(--radius-md);
      font-size: 12px;
      font-weight: 600;
      color: var(--accent);
      text-transform: uppercase;
      letter-spacing: 0.05em;
      margin-bottom: 16px;
    }

    .section-title {
      font-size: 40px;
      font-weight: 700;
      color: var(--text-strong);
      letter-spacing: -0.02em;
      margin: 0 0 16px;
    }

    .section-desc {
      font-size: 16px;
      color: var(--muted);
      max-width: 600px;
      margin: 0 auto;
      line-height: 1.6;
    }

    .features-grid {
      max-width: 1200px;
      margin: 0 auto;
      display: grid;
      grid-template-columns: repeat(2, 1fr);
      gap: 24px;
    }

    @media (max-width: 768px) {
      .features-grid {
        grid-template-columns: 1fr;
      }
    }

    .feature-card {
      padding: 32px;
      background: var(--card);
      border: 1px solid var(--border);
      border-radius: var(--radius-lg);
      transition: all 0.2s ease;
    }

    .feature-card:hover {
      border-color: var(--border-hover);
      transform: translateY(-2px);
    }

    .feature-icon {
      width: 48px;
      height: 48px;
      display: flex;
      align-items: center;
      justify-content: center;
      background: var(--accent-subtle);
      border-radius: var(--radius-md);
      margin-bottom: 20px;
    }

    .feature-icon svg {
      width: 24px;
      height: 24px;
      color: var(--accent);
    }

    .feature-title {
      font-size: 20px;
      font-weight: 600;
      color: var(--text-strong);
      margin: 0 0 12px;
      letter-spacing: -0.01em;
    }

    .feature-desc {
      font-size: 14px;
      color: var(--muted);
      line-height: 1.6;
      margin: 0;
    }

    /* Agents */
    .agents {
      padding: 0 24px 100px;
      background: var(--bg);
    }

    .agents-grid {
      max-width: 1200px;
      margin: 0 auto;
      display: grid;
      grid-template-columns: repeat(3, 1fr);
      gap: 16px;
    }

    @media (max-width: 900px) {
      .agents-grid {
        grid-template-columns: repeat(2, 1fr);
      }
    }

    @media (max-width: 600px) {
      .agents-grid {
        grid-template-columns: 1fr;
      }
    }

    .agent-card {
      padding: 20px;
      background: var(--card);
      border: 1px solid var(--border);
      border-radius: var(--radius-md);
      cursor: pointer;
      transition: all 0.15s ease;
    }

    .agent-card:hover {
      background: var(--bg-hover);
      border-color: var(--border-hover);
    }

    .agent-header {
      display: flex;
      align-items: center;
      gap: 12px;
      margin-bottom: 12px;
    }

    .agent-avatar {
      width: 36px;
      height: 36px;
      display: flex;
      align-items: center;
      justify-content: center;
      background: var(--accent-subtle);
      border-radius: var(--radius-md);
      font-size: 16px;
    }

    .agent-name {
      font-size: 14px;
      font-weight: 600;
      color: var(--text-strong);
    }

    .agent-desc {
      font-size: 13px;
      color: var(--muted);
      line-height: 1.5;
      margin: 0;
    }

    /* CTA */
    .cta {
      padding: 100px 24px;
      background: linear-gradient(180deg, var(--bg) 0%, var(--bg-accent) 100%);
      text-align: center;
    }

    .cta-inner {
      max-width: 600px;
      margin: 0 auto;
    }

    .cta-title {
      font-size: 36px;
      font-weight: 700;
      color: var(--text-strong);
      letter-spacing: -0.02em;
      margin: 0 0 16px;
    }

    .cta-desc {
      font-size: 16px;
      color: var(--muted);
      margin: 0 0 32px;
      line-height: 1.6;
    }

    /* Footer */
    .footer {
      padding: 48px 24px;
      background: var(--bg-accent);
      border-top: 1px solid var(--border);
    }

    .footer-inner {
      max-width: 1200px;
      margin: 0 auto;
      display: flex;
      justify-content: space-between;
      align-items: flex-start;
      gap: 48px;
      flex-wrap: wrap;
    }

    .footer-brand {
      max-width: 300px;
    }

    .footer-logo {
      display: flex;
      align-items: center;
      gap: 8px;
      margin-bottom: 16px;
    }

    .footer-logo svg {
      width: 28px;
      height: 28px;
      color: var(--accent);
    }

    .footer-logo-text {
      font-size: 16px;
      font-weight: 700;
      color: var(--text-strong);
    }

    .footer-tagline {
      font-size: 13px;
      color: var(--muted);
      line-height: 1.6;
      margin: 0;
    }

    .footer-links {
      display: flex;
      gap: 80px;
    }

    @media (max-width: 768px) {
      .footer-links {
        gap: 40px;
      }
    }

    .footer-col h4 {
      font-size: 13px;
      font-weight: 600;
      color: var(--text-strong);
      margin: 0 0 16px;
      text-transform: uppercase;
      letter-spacing: 0.05em;
    }

    .footer-col ul {
      list-style: none;
      padding: 0;
      margin: 0;
    }

    .footer-col li {
      margin-bottom: 10px;
    }

    .footer-col a {
      font-size: 13px;
      color: var(--muted);
      text-decoration: none;
      transition: color 0.15s ease;
    }

    .footer-col a:hover {
      color: var(--text-strong);
    }

    .footer-bottom {
      max-width: 1200px;
      margin: 32px auto 0;
      padding-top: 24px;
      border-top: 1px solid var(--border);
      display: flex;
      justify-content: space-between;
      align-items: center;
      flex-wrap: wrap;
      gap: 16px;
    }

    .footer-copyright {
      font-size: 12px;
      color: var(--muted);
    }

    .footer-legal {
      display: flex;
      gap: 24px;
    }

    .footer-legal a {
      font-size: 12px;
      color: var(--muted);
      text-decoration: none;
      transition: color 0.15s ease;
    }

    .footer-legal a:hover {
      color: var(--text-strong);
    }
  `

  render() {
    return html`
      <!-- Navigation -->
      <nav class="nav">
        <div class="nav-inner">
          <div class="nav-left">
            <div class="nav-logo" @click=${() => navigate('')}>
              <svg viewBox="0 0 56 56" style="width:36px;height:36px;color:var(--accent);">
                <path d="M1890 7776c-165 -46 -338 -179 -462 -356 -217 -309 -439 -926 -572 -1590 -72 -358 -110 -663 -132 -1050 l-7 -124 -45 -61c-208 -282 -364 -669 -418 -1040 -25 -167 -25 -505 0 -670 31 -208 106 -435 198 -605 75 -137 206 -320 230 -320 6 0 27 14 48 30 63 50 199 117 295 146 123 36 320 44 462 19 269 -48 489 -145 690 -303 292 -230 483 -600 483 -933 l0 -85 163 -32c342 -68 565 -87 1007 -87 445 0 613 14 1021 85 l126 23 6 126c24 591 511 1096 1165 1207 138 24 326 15 447 -21 113 -33 176 -62 255 -115 34 -23 63 -40 64 -38 85 104 191 279 254 424 115 261 164 508 164 819 0 301 -49 564 -160 855 -76 201 -223 461 -358 634 -39 49 -54 77 -54 100 0 147 -112 787 -196 1120 -173 688 -448 1316 -688 1574 -114 122 -287 234 -413 267 -129 34 -292 11 -395 -54 -103 -65 -163 -141 -323 -406 -118 -196 -294 -575 -395 -850 -18 -49 -36 -93 -39 -96 -3 -4 -40 -1 -83 7 -145 25 -284 35 -513 34 -211 0 -384 -13 -610 -46 l-40 -6 -39 99c-22 54 -76 181 -121 283 -249 562 -467 901 -641 992 -103 55 -265 74 -374 44z m2173 -3657c197 -24 459 -95 503 -135 34 -33 32 -77 -6 -116 -36 -36 -58 -35 -194 6 -381 114 -807 114 -1183 -1 -116 -36 -158 -35 -189 1 -54 62 -13 120 112 159 43 14 131 36 194 50 100 23 169 32 355 51 60 6 315 -3 408 -15z m-1842 -263c42 -38 146 -76 209 -76 56 0 87 -15 110 -52 61 -100 -30 -201 -230 -255 -90 -25 -309 -24 -399 1 -153 42 -241 114 -241 197 0 115 198 216 429 218 82 1 84 0 122 -33z m3424 9c85 -22 161 -63 201 -107 29 -33 34 -45 34 -88 0 -43 -5 -55 -35 -89 -152 -168 -633 -173 -797 -8 -42 42 -54 83 -38 131 16 50 38 64 126 80 90 15 161 45 195 83 24 25 25 25 139 20 63 -3 142 -13 175 -22z m-634 -704c34 -34 36 -58 10 -109 -65 -126 -223 -247 -376 -287 -35 -9 -102 -15 -166 -15 -95 0 -127 5 -285 47 l-179 46 -225 1 -225 0 -155 -42c-191 -53 -350 -66 -460 -37 -102 26 -196 81 -277 161 -110 107 -135 187 -75 234 13 11 40 20 59 20 36 0 45 -7 120 -97 126 -152 318 -190 568 -115 319 97 556 94 960 -13 84 -22 116 -26 201 -23 91 3 107 7 167 37 64 33 148 107 187 165 40 62 104 73 151 27z m-1023 -617c92 -19 179 -48 247 -82 120 -60 249 -170 298 -253 l27 -46 -20 -66c-39 -125 -149 -262 -286 -352 -170 -113 -410 -162 -613 -125 -291 53 -551 266 -613 502 -12 46 -11 48 24 101 99 149 341 289 561 326 90 15 291 12 375 -5z m-1562 -1589c-278 -50 -513 -295 -614 -641 -56 -194 -90 -441 -73 -534 20 -107 87 -186 187 -219 45 -15 190 -16 252 -2 l41 9 54 -57c33 -34 78 -68 113 -86 53 -26 71 -29 150 -29 70 0 104 5 152 24 34 13 67 29 74 37 9 11 19 7 53 -21 112 -92 282 -112 410 -48 43 22 114 91 142 138 24 42 25 42 93 48 208 17 310 160 281 394 -36 291 -177 541 -404 717 -191 149 -397 237 -631 271 -122 18 -176 18 -280 -1z m5095 5c-289 -33 -575 -172 -768 -373 -156 -164 -250 -352 -288 -577 -35 -214 5 -338 132 -400 46 -23 80 -32 124 -34 60 -2 61 -3 81 -41 32 -60 88 -112 159 -147 57 -28 74 -32 145 -31 89 0 166 28 238 86 l38 30 76 -38c72 -37 81 -39 167 -39 83 0 98 3 158 33 46 22 83 51 117 88 l49 56 70 -12c96 -17 226 -9 277 17 50 26 96 75 126 133 22 41 24 57 23 175 -2 257 -91 572 -214 749 -87 127 -223 238 -348 283 -59 22 -234 55 -272 52 -8 -1 -49 -5 -90 -10z" fill="currentColor"/>
              </svg>
              <span class="nav-logo-text">TinyIoTHub</span>
            </div>
            <div class="nav-links">
              <a class="nav-link" href="/marketplace">市场</a>
              <a class="nav-link" href="/docs">文档</a>
            </div>
          </div>
          <div class="nav-right">
            <a class="nav-github" href="https://github.com" target="_blank">
              <svg width="20" height="20" viewBox="0 0 24 24" fill="currentColor">
                <path d="M12 0c-6.626 0-12 5.373-12 12 0 5.302 3.438 9.8 8.207 11.387.599.111.793-.261.793-.577v-2.234c-3.338.726-4.033-1.416-4.033-1.416-.546-1.387-1.333-1.756-1.333-1.756-1.089-.745.083-.729.083-.729 1.205.084 1.839 1.237 1.839 1.237 1.07 1.834 2.807 1.304 3.492.997.107-.775.418-1.305.762-1.604-2.665-.305-5.467-1.334-5.467-5.931 0-1.311.469-2.381 1.236-3.221-.124-.303-.535-1.524.117-3.176 0 0 1.008-.322 3.301 1.23.957-.266 1.983-.399 3.003-.404 1.02.005 2.047.138 3.006.404 2.291-1.552 3.297-1.23 3.297-1.23.653 1.653.242 2.874.118 3.176.77.84 1.235 1.911 1.235 3.221 0 4.609-2.807 5.624-5.479 5.921.43.372.823 1.102.823 2.222v3.293c0 .319.192.694.801.576 4.765-1.589 8.199-6.086 8.199-11.386 0-6.627-5.373-12-12-12z"/>
              </svg>
            </a>
            <button class="nav-btn ghost" @click=${() => navigate('signin')}>登录</button>
            <button class="nav-btn primary" @click=${() => navigate('tenant/register')}>注册</button>
          </div>
        </div>
      </nav>

      <!-- Hero -->
      <section class="hero">
        <div class="hero-badge">
          <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
            <path stroke-linecap="round" stroke-linejoin="round" d="M9.813 15.904L9 18.75l-.813-2.846a4.5 4.5 0 00-3.09-3.09L2.25 12l2.846-.813a4.5 4.5 0 003.09-3.09L9 5.25l.813 2.846a4.5 4.5 0 003.09 3.09L15.75 12l-2.846.813a4.5 4.5 0 00-3.09 3.09zM18.259 8.715L18 9.75l-.259-1.035a3.375 3.375 0 00-2.455-2.456L14.25 6l1.036-.259a3.375 3.375 0 002.455-2.456L18 2.25l.259 1.035a3.375 3.375 0 002.456 2.456L21.75 6l-1.035.259a3.375 3.375 0 00-2.456 2.456zM16.894 20.567L16.5 21.75l-.394-1.183a2.25 2.25 0 00-1.423-1.423L13.5 18.75l1.183-.394a2.25 2.25 0 001.423-1.423l.394-1.183.394 1.183a2.25 2.25 0 001.423 1.423l1.183.394-1.183.394a2.25 2.25 0 00-1.423 1.423z"/>
          </svg>
          Edge Intelligence Agent
        </div>
        <h1 class="hero-title">
          <span class="gradient">智能边缘网关</span><br/>
          连接万物，驱动未来
        </h1>
        <p class="hero-desc">
          TinyIoTHub 是一款面向工业物联网的高性能边缘计算网关平台，支持多协议设备接入、实时数据处理与智能告警
        </p>
        <div class="hero-btns">
          <button class="btn primary" @click=${() => navigate('tenant/register')}>
            立即体验
            <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
              <path stroke-linecap="round" stroke-linejoin="round" d="M13.5 4.5L21 12m0 0l-7.5 7.5M21 12H3"/>
            </svg>
          </button>
          <button class="btn secondary" @click=${() => navigate('signin')}>
            登录控制台
          </button>
        </div>
      </section>

      <!-- Protocols -->
      <section class="protocols">
        <div class="protocols-inner">
          <div class="protocol-badge">
            <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
              <path stroke-linecap="round" stroke-linejoin="round" d="M5.25 14.25h13.5m-13.5 0a3 3 0 01-3-3m3 3a3 3 0 100 6h13.5a3 3 0 100-6m-16.5-3a3 3 0 013-3h13.5a3 3 0 013 3m-19.5 0a4.5 4.5 0 01.9-2.7L5.737 5.1a3.375 3.375 0 012.7-1.35h7.126c1.062 0 2.062.5 2.7 1.35l2.587 3.45a4.5 4.5 0 01.9 2.7m0 0a3 3 0 01-3 3m0 3h.008v.008h-.008v-.008zm0-6h.008v.008h-.008v-.008zm-3 6h.008v.008h-.008v-.008zm0-6h.008v.008h-.008v-.008z"/>
            </svg>
            Modbus
          </div>
          <div class="protocol-badge">
            <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
              <path stroke-linecap="round" stroke-linejoin="round" d="M15 10.5a3 3 0 11-6 0 3 3 0 016 0z"/>
              <path stroke-linecap="round" stroke-linejoin="round" d="M19.5 10.5c0 7.142-7.5 11.25-7.5 11.25S4.5 17.642 4.5 10.5a7.5 7.5 0 1115 0z"/>
            </svg>
            ONVIF
          </div>
          <div class="protocol-badge">
            <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
              <path stroke-linecap="round" stroke-linejoin="round" d="M8.288 15.038a5.25 5.25 0 017.424 0M5.106 11.856c3.807-3.808 9.98-3.808 13.788 0M1.924 8.674c5.565-5.565 14.587-5.565 20.152 0M12.53 18.22l-.53.53-.53-.53a.75.75 0 011.06 0z"/>
            </svg>
            SNMP
          </div>
          <div class="protocol-badge">
            <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
              <path stroke-linecap="round" stroke-linejoin="round" d="M8.25 3v1.5M4.5 8.25H3m18 0h-1.5M4.5 12H3m18 0h-1.5m-15 3.75H3m18 0h-1.5M8.25 19.5V21M12 3v1.5m0 15V21m3.75-18v1.5m0 15V21m-9-1.5h10.5a2.25 2.25 0 002.25-2.25V6.75a2.25 2.25 0 00-2.25-2.25H6.75A2.25 2.25 0 004.5 6.75v10.5a2.25 2.25 0 002.25 2.25zm.75-12h9v9h-9v-9z"/>
            </svg>
            MQTT
          </div>
        </div>
      </section>

      <!-- Stats -->
      <section class="stats">
        <div class="stats-inner">
          <div class="stat-item">
            <div class="stat-value">99.99%</div>
            <div class="stat-label">系统可用性</div>
          </div>
          <div class="stat-item">
            <div class="stat-value">9999+</div>
            <div class="stat-label">并发设备接入</div>
          </div>
          <div class="stat-item">
            <div class="stat-value">&lt;50ms</div>
            <div class="stat-label">平均响应延迟</div>
          </div>
          <div class="stat-item">
            <div class="stat-value">7×24</div>
            <div class="stat-label">全天候监控</div>
          </div>
        </div>
      </section>

      <!-- Features -->
      <section class="features">
        <div class="section-header">
          <span class="section-tag">核心功能</span>
          <h2 class="section-title">为什么选择 TinyIoTHub？</h2>
          <p class="section-desc">
            强大的边缘计算能力，多协议支持，以及智能化的运维管理
          </p>
        </div>
        <div class="features-grid">
          <div class="feature-card">
            <div class="feature-icon">
              <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
                <path stroke-linecap="round" stroke-linejoin="round" d="M3.75 13.5l10.5-11.25L12 10.5h8.25L9.75 21.75 12 13.5H3.75z"/>
              </svg>
            </div>
            <h3 class="feature-title">边缘计算</h3>
            <p class="feature-desc">支持在边缘网关本地执行数据处理、协议转换和逻辑运算，减少云端依赖，降低延迟</p>
          </div>
          <div class="feature-card">
            <div class="feature-icon">
              <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
                <path stroke-linecap="round" stroke-linejoin="round" d="M7.5 21L3 16.5m0 0L7.5 12M3 16.5h13.5m0-13.5L21 7.5m0 0L16.5 12M21 7.5H7.5"/>
              </svg>
            </div>
            <h3 class="feature-title">多协议适配</h3>
            <p class="feature-desc">同时支持 Modbus、ONVIF、SNMP、MQTT 等主流工业协议，灵活适配各种设备</p>
          </div>
          <div class="feature-card">
            <div class="feature-icon">
              <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
                <path stroke-linecap="round" stroke-linejoin="round" d="M3 13.125C3 12.504 3.504 12 4.125 12h2.25c.621 0 1.125.504 1.125 1.125v6.75C7.5 20.496 6.996 21 6.375 21h-2.25A1.125 1.125 0 013 19.875v-6.75zM9.75 8.625c0-.621.504-1.125 1.125-1.125h2.25c.621 0 1.125.504 1.125 1.125v11.25c0 .621-.504 1.125-1.125 1.125h-2.25a1.125 1.125 0 01-1.125-1.125V8.625zM16.5 4.125c0-.621.504-1.125 1.125-1.125h2.25C20.496 3 21 3.504 21 4.125v15.75c0 .621-.504 1.125-1.125 1.125h-2.25a1.125 1.125 0 01-1.125-1.125V4.125z"/>
              </svg>
            </div>
            <h3 class="feature-title">实时监控</h3>
            <p class="feature-desc">可视化仪表盘，实时展示设备状态、数据趋势和告警信息，掌握全局动态</p>
          </div>
          <div class="feature-card">
            <div class="feature-icon">
              <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
                <path stroke-linecap="round" stroke-linejoin="round" d="M14.857 17.082a23.848 23.848 0 005.454-1.31A8.967 8.967 0 0118 9.75v-.7V9A6 6 0 006 9v.75a8.967 8.967 0 01-2.312 6.022c1.733.64 3.56 1.085 5.455 1.31m5.714 0a24.255 24.255 0 01-5.714 0m5.714 0a3 3 0 11-5.714 0"/>
              </svg>
            </div>
            <h3 class="feature-title">智能告警</h3>
            <p class="feature-desc">多级别告警规则，灵活的通知策略，支持邮件、短信、Webhook 等多种方式</p>
          </div>
        </div>
      </section>

      <!-- AI Agents -->
      <section class="agents">
        <div class="section-header">
          <span class="section-tag">AI Agent</span>
          <h2 class="section-title">智能运维助手</h2>
          <p class="section-desc">
            基于大语言模型的智能助手，帮助您更高效地管理和运维 IoT 设备
          </p>
        </div>
        <div class="agents-grid">
          <div class="agent-card">
            <div class="agent-header">
              <div class="agent-avatar">🔧</div>
              <span class="agent-name">设备诊断助手</span>
            </div>
            <p class="agent-desc">自动分析设备异常，提供诊断建议和解决方案</p>
          </div>
          <div class="agent-card">
            <div class="agent-header">
              <div class="agent-avatar">📊</div>
              <span class="agent-name">数据分析师</span>
            </div>
            <p class="agent-desc">智能分析设备数据，发现趋势和潜在问题</p>
          </div>
          <div class="agent-card">
            <div class="agent-header">
              <div class="agent-avatar">⚡</div>
              <span class="agent-name">性能优化师</span>
            </div>
            <p class="agent-desc">监控系统性能，提供优化建议和容量规划</p>
          </div>
          <div class="agent-card">
            <div class="agent-header">
              <div class="agent-avatar">🔒</div>
              <span class="agent-name">安全审计员</span>
            </div>
            <p class="agent-desc">实时监控安全事件，及时发现和响应威胁</p>
          </div>
          <div class="agent-card">
            <div class="agent-header">
              <div class="agent-avatar">📝</div>
              <span class="agent-name">日志分析员</span>
            </div>
            <p class="agent-desc">自动分析日志数据，快速定位问题根因</p>
          </div>
          <div class="agent-card">
            <div class="agent-header">
              <div class="agent-avatar">🤖</div>
              <span class="agent-name">自动化工程师</span>
            </div>
            <p class="agent-desc">编排自动化任务，减少人工干预和重复工作</p>
          </div>
        </div>
      </section>

      <!-- CTA -->
      <section class="cta">
        <div class="cta-inner">
          <h2 class="cta-title">立即开始使用</h2>
          <p class="cta-desc">
            几分钟内完成注册，即可体验完整的 IoT 边缘网关功能
          </p>
          <div class="hero-btns">
            <button class="btn primary" @click=${() => navigate('tenant/register')}>
              免费试用
              <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
                <path stroke-linecap="round" stroke-linejoin="round" d="M13.5 4.5L21 12m0 0l-7.5 7.5M21 12H3"/>
              </svg>
            </button>
          </div>
        </div>
      </section>

      <!-- Footer -->
      <footer class="footer">
        <div class="footer-inner">
          <div class="footer-brand">
            <div class="footer-logo">
              <svg viewBox="0 0 56 56" style="width:28px;height:28px;color:var(--accent);">
                <path d="M1890 7776c-165 -46 -338 -179 -462 -356 -217 -309 -439 -926 -572 -1590 -72 -358 -110 -663 -132 -1050 l-7 -124 -45 -61c-208 -282 -364 -669 -418 -1040 -25 -167 -25 -505 0 -670 31 -208 106 -435 198 -605 75 -137 206 -320 230 -320 6 0 27 14 48 30 63 50 199 117 295 146 123 36 320 44 462 19 269 -48 489 -145 690 -303 292 -230 483 -600 483 -933 l0 -85 163 -32c342 -68 565 -87 1007 -87 445 0 613 14 1021 85 l126 23 6 126c24 591 511 1096 1165 1207 138 24 326 15 447 -21 113 -33 176 -62 255 -115 34 -23 63 -40 64 -38 85 104 191 279 254 424 115 261 164 508 164 819 0 301 -49 564 -160 855 -76 201 -223 461 -358 634 -39 49 -54 77 -54 100 0 147 -112 787 -196 1120 -173 688 -448 1316 -688 1574 -114 122 -287 234 -413 267 -129 34 -292 11 -395 -54 -103 -65 -163 -141 -323 -406 -118 -196 -294 -575 -395 -850 -18 -49 -36 -93 -39 -96 -3 -4 -40 -1 -83 7 -145 25 -284 35 -513 34 -211 0 -384 -13 -610 -46 l-40 -6 -39 99c-22 54 -76 181 -121 283 -249 562 -467 901 -641 992 -103 55 -265 74 -374 44z m2173 -3657c197 -24 459 -95 503 -135 34 -33 32 -77 -6 -116 -36 -36 -58 -35 -194 6 -381 114 -807 114 -1183 -1 -116 -36 -158 -35 -189 1 -54 62 -13 120 112 159 43 14 131 36 194 50 100 23 169 32 355 51 60 6 315 -3 408 -15z m-1842 -263c42 -38 146 -76 209 -76 56 0 87 -15 110 -52 61 -100 -30 -201 -230 -255 -90 -25 -309 -24 -399 1 -153 42 -241 114 -241 197 0 115 198 216 429 218 82 1 84 0 122 -33z m3424 9c85 -22 161 -63 201 -107 29 -33 34 -45 34 -88 0 -43 -5 -55 -35 -89 -152 -168 -633 -173 -797 -8 -42 42 -54 83 -38 131 16 50 38 64 126 80 90 15 161 45 195 83 24 25 25 25 139 20 63 -3 142 -13 175 -22z m-634 -704c34 -34 36 -58 10 -109 -65 -126 -223 -247 -376 -287 -35 -9 -102 -15 -166 -15 -95 0 -127 5 -285 47 l-179 46 -225 1 -225 0 -155 -42c-191 -53 -350 -66 -460 -37 -102 26 -196 81 -277 161 -110 107 -135 187 -75 234 13 11 40 20 59 20 36 0 45 -7 120 -97 126 -152 318 -190 568 -115 319 97 556 94 960 -13 84 -22 116 -26 201 -23 91 3 107 7 167 37 64 33 148 107 187 165 40 62 104 73 151 27z m-1023 -617c92 -19 179 -48 247 -82 120 -60 249 -170 298 -253 l27 -46 -20 -66c-39 -125 -149 -262 -286 -352 -170 -113 -410 -162 -613 -125 -291 53 -551 266 -613 502 -12 46 -11 48 24 101 99 149 341 289 561 326 90 15 291 12 375 -5z m-1562 -1589c-278 -50 -513 -295 -614 -641 -56 -194 -90 -441 -73 -534 20 -107 87 -186 187 -219 45 -15 190 -16 252 -2 l41 9 54 -57c33 -34 78 -68 113 -86 53 -26 71 -29 150 -29 70 0 104 5 152 24 34 13 67 29 74 37 9 11 19 7 53 -21 112 -92 282 -112 410 -48 43 22 114 91 142 138 24 42 25 42 93 48 208 17 310 160 281 394 -36 291 -177 541 -404 717 -191 149 -397 237 -631 271 -122 18 -176 18 -280 -1z m5095 5c-289 -33 -575 -172 -768 -373 -156 -164 -250 -352 -288 -577 -35 -214 5 -338 132 -400 46 -23 80 -32 124 -34 60 -2 61 -3 81 -41 32 -60 88 -112 159 -147 57 -28 74 -32 145 -31 89 0 166 28 238 86 l38 30 76 -38c72 -37 81 -39 167 -39 83 0 98 3 158 33 46 22 83 51 117 88 l49 56 70 -12c96 -17 226 -9 277 17 50 26 96 75 126 133 22 41 24 57 23 175 -2 257 -91 572 -214 749 -87 127 -223 238 -348 283 -59 22 -234 55 -272 52 -8 -1 -49 -5 -90 -10z" fill="currentColor"/>
              </svg>
              <span class="footer-logo-text">TinyIoTHub</span>
            </div>
            <p class="footer-tagline">
              高性能 IoT 边缘网关平台，支持多协议设备接入与智能运维
            </p>
          </div>
          <div class="footer-links">
            <div class="footer-col">
              <h4>产品</h4>
              <ul>
                <li><a href="/marketplace">市场</a></li>
                <li><a href="/docs">文档</a></li>
                <li><a href="/pricing">定价</a></li>
              </ul>
            </div>
            <div class="footer-col">
              <h4>支持</h4>
              <ul>
                <li><a href="/help">帮助中心</a></li>
                <li><a href="/contact">联系我们</a></li>
                <li><a href="/faq">常见问题</a></li>
              </ul>
            </div>
            <div class="footer-col">
              <h4>资源</h4>
              <ul>
                <li><a href="/blog">博客</a></li>
                <li><a href="/github">GitHub</a></li>
                <li><a href="/community">社区</a></li>
              </ul>
            </div>
          </div>
        </div>
        <div class="footer-bottom">
          <span class="footer-copyright">© 2024 TinyIoTHub. 京ICP备XXXXXXXX号-1</span>
          <div class="footer-legal">
            <a href="/privacy">隐私政策</a>
            <a href="/terms">服务条款</a>
          </div>
        </div>
      </footer>
    `
  }
}

declare global {
  interface HTMLElementTagNameMap {
    'home-page': HomePage
  }
}
