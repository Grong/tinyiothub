import { LitElement, html, css } from 'lit'
import { customElement, state } from 'lit/decorators.js'
import { authApi, type LoginRequest } from '../services/auth'
import { setAuth } from '../stores/auth-store'
import { navigate } from '../lib/navigate'

@customElement('signin-page')
export class SigninPage extends LitElement {
  static styles = css`
    :host {
      position: fixed;
      inset: 0;
      z-index: 100;
      display: flex;
      min-height: 100vh;
      background: var(--bg);
    }

    /* Left side - Branding */
    .brand-side {
      display: none;
      width: 50%;
      background: linear-gradient(135deg, var(--bg) 0%, var(--bg-accent) 100%);
      position: relative;
      overflow: hidden;
    }

    @media (min-width: 1024px) {
      .brand-side {
        display: flex;
      }
    }

    .brand-content {
      position: relative;
      z-index: 1;
      display: flex;
      flex-direction: column;
      justify-content: center;
      padding: 64px;
      color: white;
    }

    .brand-logo {
      display: flex;
      align-items: center;
      gap: 16px;
      margin-bottom: 32px;
    }

    .brand-logo svg {
      width: 48px;
      height: 48px;
    }

    .brand-logo h1 {
      font-size: 36px;
      font-weight: 700;
      margin: 0;
      letter-spacing: -0.02em;
    }

    .brand-tagline {
      font-size: 20px;
      line-height: 1.6;
      color: var(--muted);
      max-width: 400px;
      margin-bottom: 48px;
    }

    .brand-features {
      display: flex;
      flex-direction: column;
      gap: 16px;
    }

    .brand-feature {
      display: flex;
      align-items: center;
      gap: 12px;
      font-size: 14px;
      color: var(--text);
    }

    .brand-feature-icon {
      width: 32px;
      height: 32px;
      border-radius: 8px;
      background: var(--accent-subtle);
      display: flex;
      align-items: center;
      justify-content: center;
      color: var(--accent);
    }

    /* Decorative orb */
    .orb {
      position: absolute;
      border-radius: 50%;
      filter: blur(80px);
      opacity: 0.3;
    }

    .orb-1 {
      width: 400px;
      height: 400px;
      background: var(--accent);
      top: -100px;
      right: -100px;
    }

    .orb-2 {
      width: 300px;
      height: 300px;
      background: var(--accent-2);
      bottom: -50px;
      left: -50px;
    }

    /* Right side - Form */
    .form-side {
      flex: 1;
      display: flex;
      align-items: center;
      justify-content: center;
      padding: 32px;
    }

    .form-container {
      width: 100%;
      max-width: 400px;
    }

    .form-header {
      text-align: center;
      margin-bottom: 32px;
    }

    .form-logo {
      width: 48px;
      height: 48px;
      margin: 0 auto 16px;
    }

    .form-title {
      font-size: 24px;
      font-weight: 700;
      color: var(--text-strong);
      margin: 0 0 8px;
    }

    .form-subtitle {
      font-size: 14px;
      color: var(--muted);
      margin: 0;
    }

    /* Form card */
    .form-card {
      background: var(--card);
      border: 1px solid var(--border);
      border-radius: var(--radius-lg);
      padding: 32px;
      animation: rise 0.25s var(--ease-out) backwards;
    }

    .form-group {
      margin-bottom: 20px;
    }

    .form-label {
      display: block;
      font-size: 13px;
      font-weight: 500;
      color: var(--text);
      margin-bottom: 8px;
    }

    .form-input {
      width: 100%;
      padding: 10px 14px;
      border: 1px solid var(--input);
      border-radius: var(--radius-md);
      background: var(--bg);
      color: var(--text);
      font-size: 14px;
      transition: border-color var(--duration-fast) ease, box-shadow var(--duration-fast) ease;
    }

    .form-input:focus {
      outline: none;
      border-color: var(--accent);
      box-shadow: 0 0 0 3px var(--accent-subtle);
    }

    .form-input::placeholder {
      color: var(--muted);
    }

    /* Error message */
    .error-message {
      background: var(--danger-subtle);
      border: 1px solid var(--danger);
      border-radius: var(--radius-md);
      padding: 12px 16px;
      margin-bottom: 20px;
      font-size: 13px;
      color: var(--danger);
      animation: fade-in 0.2s ease;
    }

    /* Submit button */
    .submit-btn {
      width: 100%;
      padding: 12px 20px;
      border: none;
      border-radius: var(--radius-md);
      background: var(--accent);
      color: var(--accent-foreground);
      font-size: 14px;
      font-weight: 600;
      cursor: pointer;
      transition: background var(--duration-fast) ease, transform var(--duration-fast) ease;
    }

    .submit-btn:hover:not(:disabled) {
      background: var(--accent-hover);
    }

    .submit-btn:active:not(:disabled) {
      transform: scale(0.98);
    }

    .submit-btn:disabled {
      opacity: 0.6;
      cursor: not-allowed;
    }

    /* Default account hint */
    .default-hint {
      text-align: center;
      margin-top: 20px;
      font-size: 12px;
      color: var(--muted);
    }

    /* Back link */
    .back-link {
      display: block;
      text-align: center;
      margin-top: 24px;
      font-size: 13px;
      color: var(--muted);
      text-decoration: none;
    }

    .back-link:hover {
      color: var(--accent);
    }
  `

  @state() username = ''
  @state() password = ''
  @state() isLoading = false
  @state() error = ''

  render() {
    return html`
      <!-- Left branding side -->
      <div class="brand-side">
        <div class="orb orb-1"></div>
        <div class="orb orb-2"></div>
        <div class="brand-content">
          <div class="brand-logo">
            <img src="/logo.svg" alt="TinyIoTHub" style="width:48px;height:48px;" />
            <h1>TinyIoTHub</h1>
          </div>
          <p class="brand-tagline">
            轻量级、高性能、企业级的物联网边缘网关系统。基于 Rust + AI 构建，为工业物联网场景提供可靠的设备接入、数据采集和边缘计算能力。
          </p>
          <div class="brand-features">
            <div class="brand-feature">
              <div class="brand-feature-icon">
                <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
                  <path stroke-linecap="round" stroke-linejoin="round" d="M5 13l4 4L19 7" />
                </svg>
              </div>
              <span>内置人工智能，智能驱动匹配</span>
            </div>
            <div class="brand-feature">
              <div class="brand-feature-icon">
                <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
                  <path stroke-linecap="round" stroke-linejoin="round" d="M5 13l4 4L19 7" />
                </svg>
              </div>
              <span>接入即自治，运行即自愈</span>
            </div>
            <div class="brand-feature">
              <div class="brand-feature-icon">
                <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
                  <path stroke-linecap="round" stroke-linejoin="round" d="M5 13l4 4L19 7" />
                </svg>
              </div>
              <span>9999+ 协议支持，开箱即用</span>
            </div>
          </div>
        </div>
      </div>

      <!-- Right form side -->
      <div class="form-side">
        <div class="form-container">
          <div class="form-header">
            <img class="form-logo" src="/logo.svg" alt="TinyIoTHub" style="width:48px;height:48px;" />
            <h2 class="form-title">欢迎回来</h2>
            <p class="form-subtitle">请登录您的账户以继续</p>
          </div>

          <div class="form-card">
            ${this.error ? html`<div class="error-message">${this.error}</div>` : ''}

            <form @submit=${this.handleSubmit}>
              <div class="form-group">
                <label class="form-label" for="username">用户名</label>
                <input
                  type="text"
                  id="username"
                  class="form-input"
                  placeholder="请输入用户名"
                  .value=${this.username}
                  @input=${this.handleUsernameInput}
                  required
                />
              </div>

              <div class="form-group">
                <label class="form-label" for="password">密码</label>
                <input
                  type="password"
                  id="password"
                  class="form-input"
                  placeholder="请输入密码"
                  .value=${this.password}
                  @input=${this.handlePasswordInput}
                  required
                />
              </div>

              <button type="submit" class="submit-btn" ?disabled=${this.isLoading}>
                ${this.isLoading ? '登录中...' : '登录'}
              </button>
            </form>

            <p class="default-hint">默认账户：admin / admin123</p>
          </div>

          <a href="/" class="back-link">← 返回首页</a>
        </div>
      </div>
    `
  }

  handleUsernameInput(e: InputEvent) {
    this.username = (e.target as HTMLInputElement).value
  }

  handlePasswordInput(e: InputEvent) {
    this.password = (e.target as HTMLInputElement).value
  }

  async handleSubmit(e: Event) {
    e.preventDefault()
    this.isLoading = true
    this.error = ''

    try {
      const credentials: LoginRequest = {
        username: this.username,
        password: this.password,
      }

      const response = await authApi.login(credentials)

      if (response.result) {
        const { accessToken, userInfo } = response.result
        setAuth(accessToken, userInfo)
        navigate('dashboard')
      }
    } catch (err: any) {
      this.error = err.message || '登录失败，请检查用户名和密码'
    } finally {
      this.isLoading = false
    }
  }
}

declare global {
  interface HTMLElementTagNameMap {
    'signin-page': SigninPage
  }
}
