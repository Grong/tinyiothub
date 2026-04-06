import { LitElement, html} from 'lit'
import { customElement, state } from 'lit/decorators.js'
import { tenantApi } from '../services/tenant'
import { navigate } from '../lib/navigate'
import '../components/logo-icon'

@customElement('register-page')
export class RegisterPage extends LitElement {
  createRenderRoot() { return this }
  

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
              <logo-icon size="48px"></logo-icon>
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
