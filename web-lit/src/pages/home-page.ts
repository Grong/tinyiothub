import { LitElement, html, css } from 'lit'
import { customElement, state } from 'lit/decorators.js'
import { navigate } from '../lib/navigate'
import { $isAuthenticated } from '../stores/auth-store'

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
      background: rgba(255, 255, 255, 0.8);
      backdrop-filter: blur(12px);
      -webkit-backdrop-filter: blur(12px);
      border-bottom: 1px solid rgba(255, 255, 255, 0.3);
      transition: transform 0.3s ease;
    }

    :host([data-scrolled="true"]) .nav {
      transform: translateY(-100%);
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
    }

    .nav-logo img {
      width: 36px;
      height: 36px;
    }

    .nav-logo-text {
      font-size: 20px;
      font-weight: 700;
      color: var(--text-strong);
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
      transition: background 0.15s ease;
    }

    .nav-link:hover {
      background: var(--bg-hover);
    }

    .nav-right {
      display: flex;
      align-items: center;
      gap: 16px;
    }

    .nav-github {
      color: var(--muted);
      transition: color 0.15s ease;
    }

    .nav-github:hover {
      color: var(--text);
    }

    .nav-github svg {
      width: 20px;
      height: 20px;
    }

    .nav-btn-text {
      font-size: 14px;
      font-weight: 500;
      color: var(--text);
      cursor: pointer;
      transition: color 0.15s ease;
    }

    .nav-btn-text:hover {
      color: var(--accent);
    }

    .nav-btn-primary {
      padding: 8px 20px;
      background: var(--accent);
      color: var(--accent-foreground);
      border: none;
      border-radius: var(--radius-md);
      font-size: 14px;
      font-weight: 600;
      cursor: pointer;
      transition: background 0.15s ease;
    }

    .nav-btn-primary:hover {
      background: var(--accent-hover);
    }

    /* Hero Section */
    .hero {
      position: relative;
      padding: 140px 24px 80px;
      text-align: center;
      overflow: hidden;
    }

    .hero-bg {
      position: absolute;
      inset: 0;
      background: linear-gradient(180deg, var(--bg-accent) 0%, var(--bg) 100%);
      z-index: 0;
    }

    .hero-glow {
      position: absolute;
      top: 0;
      left: 50%;
      transform: translateX(-50%);
      width: 800px;
      height: 500px;
      background: radial-gradient(ellipse, rgba(59, 130, 246, 0.15) 0%, transparent 70%);
      pointer-events: none;
    }

    .hero-content {
      position: relative;
      z-index: 1;
      max-width: 900px;
      margin: 0 auto;
    }

    .hero-badge {
      display: inline-flex;
      align-items: center;
      gap: 8px;
      padding: 8px 16px;
      background: rgba(139, 92, 246, 0.1);
      border: 1px solid rgba(139, 92, 246, 0.3);
      border-radius: var(--radius-full);
      font-size: 14px;
      color: #8b5cf6;
      margin-bottom: 24px;
    }

    .hero-badge-dot {
      width: 8px;
      height: 8px;
      background: #a78bfa;
      border-radius: 50%;
      animation: pulse 2s infinite;
    }

    @keyframes pulse {
      0%, 100% { opacity: 1; }
      50% { opacity: 0.5; }
    }

    .hero-title {
      font-size: 56px;
      font-weight: 700;
      color: var(--text-strong);
      margin: 0 0 24px;
      letter-spacing: -0.03em;
      line-height: 1.1;
    }

    .hero-title-gradient {
      background: linear-gradient(135deg, #3b82f6 0%, #6366f1 50%, #8b5cf6 100%);
      -webkit-background-clip: text;
      -webkit-text-fill-color: transparent;
      background-clip: text;
    }

    .hero-desc {
      font-size: 20px;
      color: var(--muted);
      margin: 0 0 48px;
      line-height: 1.6;
      max-width: 700px;
      margin-left: auto;
      margin-right: auto;
    }

    .hero-actions {
      display: flex;
      gap: 16px;
      justify-content: center;
      flex-wrap: wrap;
    }

    .btn-primary {
      display: inline-flex;
      align-items: center;
      gap: 8px;
      padding: 16px 32px;
      background: linear-gradient(135deg, #3b82f6 0%, #2563eb 100%);
      color: white;
      border: none;
      border-radius: var(--radius-lg);
      font-size: 16px;
      font-weight: 600;
      cursor: pointer;
      text-decoration: none;
      box-shadow: 0 4px 14px rgba(59, 130, 246, 0.4);
      transition: all 0.2s ease;
    }

    .btn-primary:hover {
      transform: translateY(-2px);
      box-shadow: 0 6px 20px rgba(59, 130, 246, 0.5);
    }

    .btn-secondary {
      display: inline-flex;
      align-items: center;
      gap: 8px;
      padding: 16px 32px;
      background: rgba(255, 255, 255, 0.6);
      backdrop-filter: blur(8px);
      color: var(--text);
      border: 1px solid rgba(255, 255, 255, 0.5);
      border-radius: var(--radius-lg);
      font-size: 16px;
      font-weight: 600;
      cursor: pointer;
      text-decoration: none;
      box-shadow: 0 4px 14px rgba(0, 0, 0, 0.1);
      transition: all 0.2s ease;
    }

    .btn-secondary:hover {
      background: rgba(255, 255, 255, 0.8);
      transform: translateY(-2px);
    }

    /* Protocols */
    .protocols {
      padding: 48px 24px;
      text-align: center;
    }

    .protocols-label {
      font-size: 14px;
      color: var(--muted);
      margin-bottom: 20px;
    }

    .protocols-list {
      display: flex;
      gap: 12px;
      justify-content: center;
      flex-wrap: wrap;
    }

    .protocol-badge {
      padding: 10px 20px;
      background: rgba(255, 255, 255, 0.6);
      backdrop-filter: blur(8px);
      border: 1px solid rgba(255, 255, 255, 0.5);
      border-radius: var(--radius-lg);
      font-size: 14px;
      font-weight: 500;
      color: var(--text);
      box-shadow: 0 2px 8px rgba(0, 0, 0, 0.05);
    }

    .protocol-badge-accent {
      background: linear-gradient(135deg, rgba(59, 130, 246, 0.1) 0%, rgba(99, 102, 241, 0.1) 100%);
      border-color: rgba(59, 130, 246, 0.3);
      background-clip: padding-box;
    }

    /* Stats Section */
    .stats {
      padding: 64px 24px;
    }

    .stats-card {
      max-width: 1100px;
      margin: 0 auto;
      padding: 32px 48px;
      background: rgba(255, 255, 255, 0.5);
      backdrop-filter: blur(16px);
      border: 1px solid rgba(255, 255, 255, 0.6);
      border-radius: var(--radius-3xl);
      box-shadow: 0 8px 32px rgba(0, 0, 0, 0.08);
    }

    .stats-grid {
      display: grid;
      grid-template-columns: repeat(4, 1fr);
      gap: 32px;
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
      font-size: 42px;
      font-weight: 700;
      color: var(--text-strong);
      letter-spacing: -0.03em;
      background: linear-gradient(135deg, #1e293b 0%, #334155 100%);
      -webkit-background-clip: text;
      -webkit-text-fill-color: transparent;
      background-clip: text;
    }

    .stat-label {
      font-size: 14px;
      color: var(--muted);
      margin-top: 8px;
    }

    /* AI Features Section */
    .ai-section {
      padding: 96px 24px;
    }

    .ai-header {
      text-align: center;
      max-width: 700px;
      margin: 0 auto 64px;
    }

    .ai-badge {
      display: inline-flex;
      align-items: center;
      gap: 8px;
      padding: 6px 14px;
      background: rgba(139, 92, 246, 0.1);
      border: 1px solid rgba(139, 92, 246, 0.3);
      border-radius: var(--radius-full);
      font-size: 14px;
      color: #8b5cf6;
      margin-bottom: 24px;
    }

    .ai-badge svg {
      width: 16px;
      height: 16px;
    }

    .ai-title {
      font-size: 42px;
      font-weight: 700;
      color: var(--text-strong);
      margin: 0 0 24px;
      letter-spacing: -0.02em;
    }

    .ai-desc {
      font-size: 18px;
      color: var(--muted);
      line-height: 1.7;
    }

    .ai-accent {
      color: #8b5cf6;
      font-weight: 600;
    }

    /* Feature Cards */
    .features-grid {
      display: grid;
      grid-template-columns: repeat(4, 1fr);
      gap: 24px;
      max-width: 1200px;
      margin: 0 auto 48px;
    }

    @media (max-width: 900px) {
      .features-grid {
        grid-template-columns: repeat(2, 1fr);
      }
    }

    .feature-card {
      background: rgba(255, 255, 255, 0.6);
      backdrop-filter: blur(12px);
      border: 1px solid rgba(255, 255, 255, 0.8);
      border-radius: var(--radius-2xl);
      padding: 24px;
      box-shadow: 0 4px 16px rgba(0, 0, 0, 0.05);
      transition: all 0.3s ease;
    }

    .feature-card:hover {
      transform: translateY(-4px);
      box-shadow: 0 8px 24px rgba(0, 0, 0, 0.1);
    }

    .feature-icon {
      width: 48px;
      height: 48px;
      border-radius: var(--radius-lg);
      display: flex;
      align-items: center;
      justify-content: center;
      margin-bottom: 16px;
    }

    .feature-icon svg {
      width: 24px;
      height: 24px;
    }

    .feature-icon.blue {
      background: linear-gradient(135deg, #3b82f6 0%, #2563eb 100%);
      color: white;
      box-shadow: 0 4px 12px rgba(59, 130, 246, 0.3);
    }

    .feature-icon.green {
      background: linear-gradient(135deg, #10b981 0%, #059669 100%);
      color: white;
      box-shadow: 0 4px 12px rgba(16, 185, 129, 0.3);
    }

    .feature-icon.purple {
      background: linear-gradient(135deg, #8b5cf6 0%, #7c3aed 100%);
      color: white;
      box-shadow: 0 4px 12px rgba(139, 92, 246, 0.3);
    }

    .feature-icon.orange {
      background: linear-gradient(135deg, #f97316 0%, #ea580c 100%);
      color: white;
      box-shadow: 0 4px 12px rgba(249, 115, 22, 0.3);
    }

    .feature-title {
      font-size: 18px;
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

    /* Agent Features */
    .agent-grid {
      display: grid;
      grid-template-columns: repeat(3, 1fr);
      gap: 24px;
      max-width: 1200px;
      margin: 0 auto;
    }

    @media (max-width: 900px) {
      .agent-grid {
        grid-template-columns: repeat(2, 1fr);
      }
    }

    .agent-card {
      background: rgba(255, 255, 255, 0.6);
      backdrop-filter: blur(12px);
      border: 1px solid rgba(255, 255, 255, 0.8);
      border-radius: var(--radius-2xl);
      padding: 32px;
      box-shadow: 0 4px 16px rgba(0, 0, 0, 0.05);
      transition: all 0.3s ease;
    }

    .agent-card:hover {
      transform: translateY(-4px);
      box-shadow: 0 8px 24px rgba(0, 0, 0, 0.1);
    }

    .agent-icon {
      width: 48px;
      height: 48px;
      border-radius: var(--radius-lg);
      display: flex;
      align-items: center;
      justify-content: center;
      margin-bottom: 20px;
      transition: transform 0.3s ease;
    }

    .agent-card:hover .agent-icon {
      transform: scale(1.1);
    }

    .agent-icon svg {
      width: 24px;
      height: 24px;
    }

    .agent-icon.blue {
      background: linear-gradient(135deg, #3b82f6 0%, #2563eb 100%);
      color: white;
      box-shadow: 0 4px 12px rgba(59, 130, 246, 0.3);
    }

    .agent-icon.green {
      background: linear-gradient(135deg, #10b981 0%, #059669 100%);
      color: white;
      box-shadow: 0 4px 12px rgba(16, 185, 129, 0.3);
    }

    .agent-icon.purple {
      background: linear-gradient(135deg, #8b5cf6 0%, #7c3aed 100%);
      color: white;
      box-shadow: 0 4px 12px rgba(139, 92, 246, 0.3);
    }

    .agent-icon.orange {
      background: linear-gradient(135deg, #f97316 0%, #ea580c 100%);
      color: white;
      box-shadow: 0 4px 12px rgba(249, 115, 22, 0.3);
    }

    .agent-icon.red {
      background: linear-gradient(135deg, #ef4444 0%, #dc2626 100%);
      color: white;
      box-shadow: 0 4px 12px rgba(239, 68, 68, 0.3);
    }

    .agent-icon.indigo {
      background: linear-gradient(135deg, #6366f1 0%, #4f46e5 100%);
      color: white;
      box-shadow: 0 4px 12px rgba(99, 102, 241, 0.3);
    }

    .agent-title {
      font-size: 20px;
      font-weight: 600;
      color: var(--text-strong);
      margin: 0 0 12px;
    }

    .agent-desc {
      font-size: 15px;
      color: var(--muted);
      margin: 0;
      line-height: 1.6;
    }

    /* CTA Section */
    .cta-section {
      padding: 96px 24px;
      position: relative;
    }

    .cta-bg {
      position: absolute;
      inset: 0;
      background: linear-gradient(135deg, rgba(59, 130, 246, 0.1) 0%, rgba(99, 102, 241, 0.1) 100%);
      z-index: 0;
    }

    .cta-card {
      position: relative;
      z-index: 1;
      max-width: 800px;
      margin: 0 auto;
      padding: 48px 64px;
      background: rgba(255, 255, 255, 0.6);
      backdrop-filter: blur(16px);
      border: 1px solid rgba(255, 255, 255, 0.5);
      border-radius: var(--radius-3xl);
      text-align: center;
      box-shadow: 0 8px 32px rgba(0, 0, 0, 0.08);
    }

    .cta-title {
      font-size: 36px;
      font-weight: 700;
      color: var(--text-strong);
      margin: 0 0 16px;
    }

    .cta-desc {
      font-size: 18px;
      color: var(--muted);
      margin: 0 0 32px;
      line-height: 1.6;
    }

    .cta-actions {
      display: flex;
      gap: 16px;
      justify-content: center;
      flex-wrap: wrap;
    }

    /* Footer */
    .footer {
      padding: 48px 24px;
      border-top: 1px solid var(--border);
    }

    .footer-inner {
      max-width: 1280px;
      margin: 0 auto;
      display: flex;
      flex-wrap: wrap;
      justify-content: space-between;
      align-items: center;
      gap: 24px;
    }

    .footer-brand {
      display: flex;
      align-items: center;
      gap: 12px;
    }

    .footer-brand img {
      width: 40px;
      height: 40px;
    }

    .footer-brand-text {
      font-size: 18px;
      font-weight: 700;
      color: var(--text-strong);
    }

    .footer-brand-sub {
      font-size: 14px;
      color: var(--muted);
    }

    .footer-links {
      display: flex;
      gap: 24px;
    }

    .footer-link {
      font-size: 14px;
      color: var(--muted);
      text-decoration: none;
      transition: color 0.15s ease;
    }

    .footer-link:hover {
      color: var(--text);
    }

    .footer-copy {
      font-size: 14px;
      color: var(--muted);
    }

    .footer-copy a {
      color: var(--muted);
      text-decoration: none;
    }

    .footer-copy a:hover {
      color: var(--text);
    }
  `

  @state() private isAuthenticated = false
  @state() private isScrolled = false

  connectedCallback() {
    super.connectedCallback()
    this.isAuthenticated = $isAuthenticated.get()
    $isAuthenticated.subscribe((value) => {
      this.isAuthenticated = value
    })
    window.addEventListener('scroll', this.handleScroll.bind(this))
  }

  disconnectedCallback() {
    super.disconnectedCallback()
    window.removeEventListener('scroll', this.handleScroll.bind(this))
  }

  handleScroll() {
    this.isScrolled = window.scrollY > 80
  }

  renderIcon(name: string) {
    const icons: Record<string, ReturnType<typeof html>> = {
      'sparkles': html`<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.5"><path d="M9.813 15.904L9 18.75l-.813-2.846a4.5 4.5 0 00-3.09-3.09L2.25 12l2.846-.813a4.5 4.5 0 003.09-3.09L9 5.25l.813 2.846a4.5 4.5 0 003.09 3.09L15.75 12l-2.846.813a4.5 4.5 0 00-3.09 3.09z"/></svg>`,
      'arrow-right': html`<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><path d="M13.5 4.5L21 12m0 0l-7.5 7.5M21 12H3"/></svg>`,
      'command': html`<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.5"><path d="M4.5 16.5c-1.5 1.26-2 5-2 5s3.74-.5 5-2c.71-.84.7-2.13-.09-2.91a2.18 2.18 0 00-2.91-.09zM12 15l-3-3a22 22 0 012-3.95A12.88 12.88 0 0122 2c0 2.72-.78 7.5-6 11a22.35 22.35 0 01-4 2z"/></svg>`,
      'chip': html`<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.5"><path d="M9.813 15.904L9 18.75l-.813-2.846a4.5 4.5 0 00-3.09-3.09L2.25 12l2.846-.813a4.5 4.5 0 003.09-3.09L9 5.25l.813 2.846a4.5 4.5 0 003.09 3.09L15.75 12l-2.846.813a4.5 4.5 0 00-3.09 3.09z"/></svg>`,
      'shield': html`<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.5"><path d="M9 12.75L11.25 15 15 9.75m-3-7.036A11.959 11.959 0 013.598 6 11.99 11.99 0 003 9.749c0 5.592 3.824 10.29 9 11.623 5.176-1.332 9-6.03 9-11.622 0-1.31-.21-2.571-.598-3.751h-.152c-3.196 0-6.1-1.248-8.25-3.285z"/></svg>`,
      'bolt': html`<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.5"><path d="M3.75 13.5l10.5-11.25L12 10.5h8.25L9.75 21.75 12 13.5H3.75z"/></svg>`,
      'cloud': html`<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.5"><path d="M2.25 15a4.5 4.5 0 004.5 4.5H18a3.75 3.75 0 001.332-7.257 3 3 0 00-3.758-3.848 5.25 5.25 0 00-9.764 0A3 3 0 003.75 9a4.5 4.5 0 00-1.5 3.5z"/></svg>`,
      'device': html`<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.5"><path d="M10.5 1.5H8.25A2.25 2.25 0 006 3.75v16.5a2.25 2.25 0 002.25 2.25h7.5A2.25 2.25 0 0018 20.25V3.75a2.25 2.25 0 00-2.25-2.25H13.5m-3 0V3h3V1.5m-3 0h3m-3 18.75h3"/></svg>`,
      'refresh': html`<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.5"><path d="M16.023 9.348h4.992v-.001M2.985 19.644v-4.992m0 0h4.992m-4.993 0l3.181 3.183a8.25 8.25 0 0013.803-3.7M4.031 9.865a8.25 8.25 0 0113.803-3.7l3.181 3.182m0-4.991v4.99"/></svg>`,
      'radio': html`<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.5"><path d="M12 21a9.004 9.004 0 008.716-6.747M12 21a9.004 9.004 0 01-8.716-6.747M12 21c2.485 0 4.5-4.03 4.5-9S14.485 3 12 3m0 18c-2.485 0-4.5-4.03-4.5-9S9.515 3 12 3m0 0a8.997 8.997 0 017.843 4.582M12 3a8.997 8.997 0 00-7.843 4.582"/></svg>`,
      'github': html`<svg viewBox="0 0 24 24" fill="currentColor"><path d="M12 2C6.477 2 2 6.484 2 12.017c0 4.425 2.865 8.18 6.839 9.504.5.092.682-.217.682-.483 0-.237-.008-.868-.013-1.703-2.782.605-3.369-1.343-3.369-1.343-.454-1.158-1.11-1.466-1.11-1.466-.908-.62.069-.608.069-.608 1.003.07 1.531 1.032 1.531 1.032.892 1.53 2.341 1.088 2.91.832.092-.647.35-1.088.636-1.338-2.22-.253-4.555-1.113-4.555-4.951 0-1.093.39-1.988 1.029-2.688-.103-.253-.446-1.272.098-2.65 0 0 .84-.27 2.75 1.026A9.564 9.564 0 0112 6.844c.85.004 1.705.115 2.504.337 1.909-1.296 2.747-1.027 2.747-1.027.546 1.379.202 2.398.1 2.651.64.7 1.028 1.595 1.028 2.688 0 3.848-2.339 4.695-4.566 4.943.359.309.678.92.678 1.855 0 1.338-.012 2.419-.012 2.747 0 .268.18.58.688.482A10.019 10.019 0 0022 12.017C22 6.484 17.522 2 12 2z"/></svg>`,
    }
    return icons[name] || html``
  }

  render() {
    return html`
      <!-- Navigation -->
      <nav class="nav">
        <div class="nav-inner">
          <div class="nav-left">
            <a href="/" class="nav-logo">
              <img src="data:image/svg+xml,<svg xmlns='http://www.w3.org/2000/svg' viewBox='0 0 24 24' fill='none' stroke='%2300d4aa' stroke-width='2'><path d='M8.25 3v1.5M4.5 8.25H3m18 0h-1.5M4.5 12H3m18 0h-1.5m-15 3.75H3m18 0h-1.5M8.25 19.5V21M12 3v1.5m0 15V21m3.75-18v1.5m0 15V21m-9-1.5h10.5a2.25 2.25 0 002.25-2.25V6.75a2.25 2.25 0 00-2.25-2.25H6.75A2.25 2.25 0 004.5 6.75v10.5a2.25 2.25 0 002.25 2.25zm.75-12h9v9h-9v-9z'/></svg>" alt="TinyIoTHub" />
              <span class="nav-logo-text">TinyIoTHub</span>
            </a>
            <div class="nav-links">
              <a href="/marketplace" class="nav-link">市场</a>
              <a href="https://docs.tinyiothub.com" target="_blank" class="nav-link">文档</a>
            </div>
          </div>
          <div class="nav-right">
            <a href="https://github.com/Grong/tinyiothub" target="_blank" class="nav-github">
              ${this.renderIcon('github')}
            </a>
            ${this.isAuthenticated ? html`
              <a href="/dashboard" class="nav-btn-primary">控制台</a>
            ` : html`
              <a href="/signin" class="nav-btn-text">登录</a>
              <a href="/signin" class="nav-btn-primary">免费试用</a>
            `}
          </div>
        </div>
      </nav>

      <!-- Hero Section -->
      <section class="hero">
        <div class="hero-bg"></div>
        <div class="hero-glow"></div>
        <div class="hero-content">
          <div class="hero-badge">
            <span class="hero-badge-dot"></span>
            内置人工智能 · 物联行业的 OpenAI
          </div>
          <h1 class="hero-title">
            构建下一代 <span class="hero-title-gradient">IoT 平台</span>
          </h1>
          <p class="hero-desc">
            轻量级、高性能，企业级的物联网边缘网关系统。基于 Rust + AI 构建，为工业物联网场景提供可靠的设备接入、数据采集和边缘计算能力。
          </p>
          <div class="hero-actions">
            <a href="/signin" class="btn-primary">
              开始免费试用
              ${this.renderIcon('arrow-right')}
            </a>
            <a href="https://docs.tinyiothub.com" target="_blank" class="btn-secondary">
              查看文档
            </a>
          </div>
        </div>
      </section>

      <!-- Protocols -->
      <section class="protocols">
        <div class="protocols-label">支持的协议</div>
        <div class="protocols-list">
          <span class="protocol-badge">Modbus RTU/TCP</span>
          <span class="protocol-badge">ONVIF 摄像头</span>
          <span class="protocol-badge">SNMP 网络设备</span>
          <span class="protocol-badge">MQTT 消息推送</span>
          <span class="protocol-badge protocol-badge-accent">9999+ 协议支持</span>
        </div>
      </section>

      <!-- Stats -->
      <section class="stats">
        <div class="stats-card">
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
      </section>

      <!-- AI Features -->
      <section class="ai-section">
        <div class="ai-header">
          <div class="ai-badge">
            ${this.renderIcon('sparkles')}
            AI 驱动的新一代边缘计算
          </div>
          <h2 class="ai-title">边缘智能体</h2>
          <p class="ai-desc">
            <span class="ai-accent">接入即自治，运行即自愈</span>
            <br/>
            AI 原生自主型边缘计算平台，将大模型驱动的智能体嵌入边缘侧，从根本上重塑设备接入与运维流程
          </p>
        </div>

        <div class="features-grid">
          <div class="feature-card">
            <div class="feature-icon blue">
              ${this.renderIcon('sparkles')}
            </div>
            <h3 class="feature-title">接入即自治</h3>
            <p class="feature-desc">自然语言描述设备，自动完成驱动匹配与生成</p>
          </div>
          <div class="feature-card">
            <div class="feature-icon green">
              ${this.renderIcon('refresh')}
            </div>
            <h3 class="feature-title">运行即自愈</h3>
            <p class="feature-desc">分级自愈机制，主动发现并修复故障</p>
          </div>
          <div class="feature-card">
            <div class="feature-icon purple">
              ${this.renderIcon('radio')}
            </div>
            <h3 class="feature-title">LoRa无线化</h3>
            <p class="feature-desc">免布线施工，改造无需停产</p>
          </div>
          <div class="feature-card">
            <div class="feature-icon orange">
              ${this.renderIcon('bolt')}
            </div>
            <h3 class="feature-title">持续进化</h3>
            <p class="feature-desc">云端驱动库与知识库不断积累</p>
          </div>
        </div>

        <div class="agent-grid">
          <div class="agent-card">
            <div class="agent-icon blue">
              ${this.renderIcon('command')}
            </div>
            <h3 class="agent-title">自然语言交互</h3>
            <p class="agent-desc">用日常语言配置设备、查询状态，无需专业背景</p>
          </div>
          <div class="agent-card">
            <div class="agent-icon green">
              ${this.renderIcon('chip')}
            </div>
            <h3 class="agent-title">智能驱动匹配</h3>
            <p class="agent-desc">AI自动匹配驱动库，无匹配则自动生成并测试验证</p>
          </div>
          <div class="agent-card">
            <div class="agent-icon purple">
              ${this.renderIcon('shield')}
            </div>
            <h3 class="agent-title">分级自愈机制</h3>
            <p class="agent-desc">L0-L3分级处理，从被动响应到主动运维</p>
          </div>
          <div class="agent-card">
            <div class="agent-icon orange">
              ${this.renderIcon('bolt')}
            </div>
            <h3 class="agent-title">心跳探针</h3>
            <p class="agent-desc">定期自检网关与子设备，提前发现隐患</p>
          </div>
          <div class="agent-card">
            <div class="agent-icon red">
              ${this.renderIcon('cloud')}
            </div>
            <h3 class="agent-title">云端协同</h3>
            <p class="agent-desc">状态上报、工单联动，知识闭环</p>
          </div>
          <div class="agent-card">
            <div class="agent-icon indigo">
              ${this.renderIcon('device')}
            </div>
            <h3 class="agent-title">LoRa无线接入</h3>
            <p class="agent-desc">内置LoRa网关，远距离低功耗免布线</p>
          </div>
        </div>
      </section>

      <!-- CTA -->
      <section class="cta-section">
        <div class="cta-bg"></div>
        <div class="cta-card">
          <h2 class="cta-title">准备好开始了吗？</h2>
          <p class="cta-desc">
            立即部署 TinyIoTHub，开启您的物联网之旅。开源免费，支持私有化部署。
          </p>
          <div class="cta-actions">
            <a href="/signin" class="btn-primary">
              免费开始使用
              ${this.renderIcon('arrow-right')}
            </a>
            <a href="https://github.com/Grong/tinyiothub" target="_blank" class="btn-secondary">
              ${this.renderIcon('github')}
              查看 GitHub
            </a>
          </div>
        </div>
      </section>

      <!-- Footer -->
      <footer class="footer">
        <div class="footer-inner">
          <div class="footer-brand">
            <img src="data:image/svg+xml,<svg xmlns='http://www.w3.org/2000/svg' viewBox='0 0 24 24' fill='none' stroke='%2300d4aa' stroke-width='2'><path d='M8.25 3v1.5M4.5 8.25H3m18 0h-1.5M4.5 12H3m18 0h-1.5m-15 3.75H3m18 0h-1.5M8.25 19.5V21M12 3v1.5m0 15V21m3.75-18v1.5m0 15V21m-9-1.5h10.5a2.25 2.25 0 002.25-2.25V6.75a2.25 2.25 0 00-2.25-2.25H6.75A2.25 2.25 0 004.5 6.75v10.5a2.25 2.25 0 002.25 2.25zm.75-12h9v9h-9v-9z'/></svg>" alt="TinyIoTHub" />
            <div>
              <div class="footer-brand-text">TinyIoTHub</div>
              <div class="footer-brand-sub">开源物联网平台</div>
            </div>
          </div>
          <div class="footer-links">
            <a href="https://github.com/Grong/tinyiothub" target="_blank" class="footer-link">GitHub</a>
            <a href="/marketplace" class="footer-link">市场</a>
            <a href="https://docs.tinyiothub.com" target="_blank" class="footer-link">文档</a>
            <a href="/signin" class="footer-link">登录</a>
          </div>
          <div class="footer-copy">
            &copy; 2026 TinyIoTHub. All rights reserved. | <a href="https://beian.miit.gov.cn/" target="_blank">粤ICP备2026029601号-2</a>
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
