import { LitElement, html, css } from 'lit'
import { customElement, state } from 'lit/decorators.js'
import { tenantApi } from '../services/tenant'
import { navigate } from '../lib/navigate'

@customElement('register-page')
export class RegisterPage extends LitElement {
  static styles = css`
    :host {
      display: block;
      min-height: 100%;
      background: var(--bg);
    }

    /* Layout */
    .container {
      display: grid;
      grid-template-columns: 1fr 1fr;
      min-height: 100vh;
    }

    @media (max-width: 900px) {
      .container {
        grid-template-columns: 1fr;
      }
      .branding-side {
        display: none;
      }
    }

    /* Branding side */
    .branding-side {
      background: linear-gradient(135deg, var(--accent) 0%, #1e40af 100%);
      padding: 48px;
      display: flex;
      flex-direction: column;
      justify-content: center;
      color: white;
    }

    .brand-content {
      max-width: 400px;
    }

    .brand-logo {
      display: flex;
      align-items: center;
      gap: 12px;
      margin-bottom: 32px;
    }

    .brand-logo svg {
      width: 48px;
      height: 48px;
    }

    .brand-name {
      font-size: 24px;
      font-weight: 700;
    }

    .brand-headline {
      font-size: 32px;
      font-weight: 700;
      line-height: 1.2;
      margin: 0 0 16px;
    }

    .brand-subheadline {
      font-size: 16px;
      opacity: 0.9;
      line-height: 1.6;
      margin: 0 0 32px;
    }

    .brand-features {
      list-style: none;
      padding: 0;
      margin: 0;
    }

    .brand-features li {
      display: flex;
      align-items: center;
      gap: 12px;
      margin-bottom: 16px;
      font-size: 15px;
    }

    .brand-features li svg {
      width: 20px;
      height: 20px;
      opacity: 0.9;
    }

    /* Form side */
    .form-side {
      padding: 48px;
      display: flex;
      flex-direction: column;
      justify-content: center;
    }

    .form-container {
      max-width: 400px;
      width: 100%;
      margin: 0 auto;
    }

    .form-header {
      margin-bottom: 32px;
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

    .form-subtitle a {
      color: var(--accent);
      text-decoration: none;
    }

    .form-subtitle a:hover {
      text-decoration: underline;
    }

    /* Form */
    .form {
      display: flex;
      flex-direction: column;
      gap: 20px;
    }

    .form-group {
      display: flex;
      flex-direction: column;
      gap: 8px;
    }

    .form-label {
      font-size: 14px;
      font-weight: 500;
      color: var(--text);
    }

    .form-input {
      padding: 12px 14px;
      border: 1px solid var(--input);
      border-radius: var(--radius-md);
      background: var(--bg);
      color: var(--text);
      font-size: 14px;
      transition: border-color var(--duration-fast) ease;
    }

    .form-input:focus {
      outline: none;
      border-color: var(--accent);
    }

    .form-input::placeholder {
      color: var(--muted);
    }

    .form-hint {
      font-size: 12px;
      color: var(--muted);
    }

    .form-error {
      font-size: 12px;
      color: var(--danger);
    }

    .form-row {
      display: grid;
      grid-template-columns: 1fr 1fr;
      gap: 16px;
    }

    @media (max-width: 500px) {
      .form-row {
        grid-template-columns: 1fr;
      }
    }

    .submit-btn {
      width: 100%;
      padding: 12px;
      background: var(--accent);
      color: var(--accent-foreground);
      border: none;
      border-radius: var(--radius-md);
      font-size: 15px;
      font-weight: 600;
      cursor: pointer;
      transition: background var(--duration-fast) ease;
      margin-top: 8px;
    }

    .submit-btn:hover:not(:disabled) {
      background: var(--accent-hover);
    }

    .submit-btn:disabled {
      opacity: 0.6;
      cursor: not-allowed;
    }

    .divider {
      display: flex;
      align-items: center;
      gap: 16px;
      margin: 8px 0;
    }

    .divider-line {
      flex: 1;
      height: 1px;
      background: var(--border);
    }

    .divider-text {
      font-size: 12px;
      color: var(--muted);
    }

    /* Alert */
    .alert {
      padding: 12px 16px;
      border-radius: var(--radius-md);
      font-size: 13px;
      margin-bottom: 20px;
    }

    .alert.error {
      background: var(--danger-subtle);
      color: var(--danger);
      border: 1px solid var(--danger);
    }

    .alert.success {
      background: var(--ok-subtle);
      color: var(--ok);
      border: 1px solid var(--ok);
    }

    /* Terms */
    .terms {
      font-size: 12px;
      color: var(--muted);
      text-align: center;
      margin-top: 16px;
    }

    .terms a {
      color: var(--accent);
      text-decoration: none;
    }

    .terms a:hover {
      text-decoration: underline;
    }
  `

  @state() name = ''
  @state() slug = ''
  @state() email = ''
  @state() password = ''
  @state() confirmPassword = ''
  @state() isLoading = false
  @state() error = ''
  @state() success = false

  validateSlug(value: string): string {
    if (!value) return ''
    if (!/^[a-z0-9-]+$/.test(value)) {
      return '只能包含小写字母、数字和连字符'
    }
    if (value.length < 3) {
      return '至少3个字符'
    }
    if (value.length > 32) {
      return '最多32个字符'
    }
    return ''
  }

  handleNameInput(e: InputEvent) {
    this.name = (e.target as HTMLInputElement).value
  }

  handleSlugInput(e: InputEvent) {
    const value = (e.target as HTMLInputElement).value.toLowerCase().replace(/[^a-z0-9-]/g, '')
    this.slug = value
  }

  handleEmailInput(e: InputEvent) {
    this.email = (e.target as HTMLInputElement).value
  }

  handlePasswordInput(e: InputEvent) {
    this.password = (e.target as HTMLInputElement).value
  }

  handleConfirmPasswordInput(e: InputEvent) {
    this.confirmPassword = (e.target as HTMLInputElement).value
  }

  async handleSubmit(e: Event) {
    e.preventDefault()
    this.error = ''

    // Validation
    if (!this.name.trim()) {
      this.error = '请输入组织名称'
      return
    }
    if (!this.slug.trim()) {
      this.error = '请输入子域名'
      return
    }
    const slugError = this.validateSlug(this.slug)
    if (slugError) {
      this.error = slugError
      return
    }
    if (!this.email.trim()) {
      this.error = '请输入邮箱'
      return
    }
    if (!/^[^\s@]+@[^\s@]+\.[^\s@]+$/.test(this.email)) {
      this.error = '请输入有效的邮箱地址'
      return
    }
    if (this.password.length < 6) {
      this.error = '密码至少6个字符'
      return
    }
    if (this.password !== this.confirmPassword) {
      this.error = '两次输入的密码不一致'
      return
    }

    this.isLoading = true
    try {
      const response = await tenantApi.register({
        name: this.name,
        slug: this.slug,
        email: this.email,
        password: this.password,
      })

      if (response.result) {
        this.success = true
        // Redirect to signin after short delay
        setTimeout(() => {
          navigate('signin')
        }, 2000)
      }
    } catch (err: any) {
      this.error = err.message || '注册失败，请稍后重试'
    } finally {
      this.isLoading = false
    }
  }

  render() {
    if (this.success) {
      return html`
        <div class="container">
          <div class="form-side">
            <div class="form-container">
              <div class="alert success">
                注册成功！正在跳转到登录页面...
              </div>
            </div>
          </div>
        </div>
      `
    }

    return html`
      <div class="container">
        <!-- Branding Side -->
        <div class="branding-side">
          <div class="brand-content">
            <div class="brand-logo">
              <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
                <path stroke-linecap="round" stroke-linejoin="round" d="M8.25 3v1.5M4.5 8.25H3m18 0h-1.5M4.5 12H3m18 0h-1.5m-15 3.75H3m18 0h-1.5M8.25 19.5V21M12 3v1.5m0 15V21m3.75-18v1.5m0 15V21m-9-1.5h10.5a2.25 2.25 0 002.25-2.25V6.75a2.25 2.25 0 00-2.25-2.25H6.75A2.25 2.25 0 004.5 6.75v10.5a2.25 2.25 0 002.25 2.25zm.75-12h9v9h-9v-9z"/>
              </svg>
              <span class="brand-name">TinyIoTHub</span>
            </div>
            <h1 class="brand-headline">快速构建您的物联网平台</h1>
            <p class="brand-subheadline">
              几分钟内完成注册，即刻体验多协议设备接入、实时数据监控与智能告警功能
            </p>
            <ul class="brand-features">
              <li>
                <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
                  <path stroke-linecap="round" stroke-linejoin="round" d="M9 12.75L11.25 15 15 9.75M21 12a9 9 0 11-18 0 9 9 0 0118 0z"/>
                </svg>
                支持 Modbus、ONVIF、SNMP、MQTT 等主流协议
              </li>
              <li>
                <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
                  <path stroke-linecap="round" stroke-linejoin="round" d="M9 12.75L11.25 15 15 9.75M21 12a9 9 0 11-18 0 9 9 0 0118 0z"/>
                </svg>
                实时数据监控与可视化仪表盘
              </li>
              <li>
                <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
                  <path stroke-linecap="round" stroke-linejoin="round" d="M9 12.75L11.25 15 15 9.75M21 12a9 9 0 11-18 0 9 9 0 0118 0z"/>
                </svg>
                智能告警引擎，支持多种通知方式
              </li>
              <li>
                <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
                  <path stroke-linecap="round" stroke-linejoin="round" d="M9 12.75L11.25 15 15 9.75M21 12a9 9 0 11-18 0 9 9 0 0118 0z"/>
                </svg>
                边缘计算能力，本地数据处理
              </li>
              <li>
                <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
                  <path stroke-linecap="round" stroke-linejoin="round" d="M9 12.75L11.25 15 15 9.75M21 12a9 9 0 11-18 0 9 9 0 0118 0z"/>
                </svg>
                企业级安全，JWT 认证与多租户隔离
              </li>
            </ul>
          </div>
        </div>

        <!-- Form Side -->
        <div class="form-side">
          <div class="form-container">
            <div class="form-header">
              <h2 class="form-title">创建账户</h2>
              <p class="form-subtitle">
                已有账户？<a href="/signin">立即登录</a>
              </p>
            </div>

            ${this.error ? html`<div class="alert error">${this.error}</div>` : ''}

            <form class="form" @submit=${this.handleSubmit}>
              <div class="form-group">
                <label class="form-label">组织名称</label>
                <input
                  type="text"
                  class="form-input"
                  placeholder="请输入组织名称"
                  .value=${this.name}
                  @input=${this.handleNameInput}
                />
              </div>

              <div class="form-group">
                <label class="form-label">子域名</label>
                <input
                  type="text"
                  class="form-input"
                  placeholder="your-org"
                  .value=${this.slug}
                  @input=${this.handleSlugInput}
                />
                <span class="form-hint">将作为您的访问地址：${this.slug || 'your-org'}.tinyiothub.com</span>
              </div>

              <div class="form-group">
                <label class="form-label">邮箱</label>
                <input
                  type="email"
                  class="form-input"
                  placeholder="name@company.com"
                  .value=${this.email}
                  @input=${this.handleEmailInput}
                />
              </div>

              <div class="form-row">
                <div class="form-group">
                  <label class="form-label">密码</label>
                  <input
                    type="password"
                    class="form-input"
                    placeholder="至少6个字符"
                    .value=${this.password}
                    @input=${this.handlePasswordInput}
                  />
                </div>
                <div class="form-group">
                  <label class="form-label">确认密码</label>
                  <input
                    type="password"
                    class="form-input"
                    placeholder="再次输入密码"
                    .value=${this.confirmPassword}
                    @input=${this.handleConfirmPasswordInput}
                  />
                </div>
              </div>

              <button
                type="submit"
                class="submit-btn"
                ?disabled=${this.isLoading}
              >
                ${this.isLoading ? '注册中...' : '创建账户'}
              </button>

              <p class="terms">
                注册即表示您同意我们的
                <a href="/terms">服务条款</a>和
                <a href="/privacy">隐私政策</a>
              </p>
            </form>
          </div>
        </div>
      </div>
    `
  }
}

declare global {
  interface HTMLElementTagNameMap {
    'register-page': RegisterPage
  }
}
