import { LitElement, html} from 'lit'
import { customElement, state } from 'lit/decorators.js'
import { authApi, type LoginRequest } from '../services/auth'
import { setAuth } from '../stores/auth-store'
import { setWorkspaces, selectWorkspace } from '../stores/workspace-store'
import { workspaceApi } from '../services/workspace'
import { navigate } from '../lib/navigate'
import '../components/logo-icon'

type LoginMethod = 'account' | 'phone' | 'wechat'

@customElement('signin-page')
export class SigninPage extends LitElement {
  createRenderRoot() { return this }
  

  @state() loginMethod: LoginMethod = 'account'
  @state() username = ''
  @state() password = ''
  @state() phone = ''
  @state() code = ''
  @state() isLoading = false
  @state() error = ''
  @state() success = ''
  @state() countdown = 0

  private countdownTimer?: number

  disconnectedCallback() {
    super.disconnectedCallback()
    if (this.countdownTimer) {
      clearInterval(this.countdownTimer)
    }
  }

  handleUsernameInput(e: InputEvent) {
    this.username = (e.target as HTMLInputElement).value
  }

  handlePasswordInput(e: InputEvent) {
    this.password = (e.target as HTMLInputElement).value
  }

  handlePhoneInput(e: InputEvent) {
    this.phone = (e.target as HTMLInputElement).value
  }

  handleCodeInput(e: InputEvent) {
    this.code = (e.target as HTMLInputElement).value
  }

  setLoginMethod(method: LoginMethod) {
    this.loginMethod = method
    this.error = ''
    this.success = ''
  }

  async sendVerificationCode() {
    if (!this.phone || !/^1[3-9]\d{9}$/.test(this.phone)) {
      this.error = '请输入有效的手机号'
      return
    }

    this.countdown = 60
    this.countdownTimer = window.setInterval(() => {
      this.countdown--
      if (this.countdown <= 0) {
        clearInterval(this.countdownTimer)
      }
    }, 1000)

    // Simulate sending code - in real app, call API
    this.success = '验证码已发送'
    this.error = ''
  }

  async handleAccountLogin(e: Event) {
    e.preventDefault()
    this.error = ''
    this.isLoading = true

    try {
      const credentials: LoginRequest = {
        username: this.username,
        password: this.password,
      }

      const response = await authApi.login(credentials)

      if (response.result) {
        const { accessToken, userInfo } = response.result
        setAuth(accessToken, userInfo)
        await this.loadWorkspaces()
        navigate('dashboard')
      }
    } catch (err: any) {
      this.error = err.message || '登录失败，请检查用户名和密码'
    } finally {
      this.isLoading = false
    }
  }

  async handlePhoneLogin(e: Event) {
    e.preventDefault()
    this.error = ''
    this.isLoading = true

    try {
      if (!this.phone || !/^1[3-9]\d{9}$/.test(this.phone)) {
        throw new Error('请输入有效的手机号')
      }
      if (!this.code || this.code.length !== 6) {
        throw new Error('请输入6位验证码')
      }

      // TODO: Implement phone login API call
      throw new Error('手机号登录功能尚未开放，请使用账号密码登录')
    } catch (err: any) {
      this.error = err.message || '登录失败'
    } finally {
      this.isLoading = false
    }
  }

  handleWeChatLogin() {
    // WeChat OAuth would redirect to WeChat authorization page
    // For demo, show placeholder
    this.error = '请使用微信扫描二维码登录'
  }

  private async loadWorkspaces() {
    try {
      const wsResp = await workspaceApi.list()
      if (wsResp.result?.length) {
        setWorkspaces(wsResp.result)
        const saved = sessionStorage.getItem('workspace-id')
        if (!saved) {
          selectWorkspace(wsResp.result[0].id)
        }
      }
    } catch {
      /* workspace loading is non-critical */
    }
  }

  renderAccountForm() {
    return html`
      <form @submit=${this.handleAccountLogin}>
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
    `
  }

  renderPhoneForm() {
    return html`
      <form @submit=${this.handlePhoneLogin}>
        <div class="form-group">
          <label class="form-label" for="phone">手机号</label>
          <input
            type="tel"
            id="phone"
            class="form-input"
            placeholder="请输入手机号"
            .value=${this.phone}
            @input=${this.handlePhoneInput}
            required
          />
        </div>

        <div class="form-group">
          <label class="form-label" for="code">验证码</label>
          <div class="phone-input-group">
            <input
              type="text"
              id="code"
              class="form-input"
              placeholder="请输入验证码"
              .value=${this.code}
              @input=${this.handleCodeInput}
              maxlength="6"
              required
            />
            <button
              type="button"
              class="send-code-btn"
              ?disabled=${this.countdown > 0}
              @click=${this.sendVerificationCode}
            >
              ${this.countdown > 0 ? `${this.countdown}s` : '发送验证码'}
            </button>
          </div>
        </div>

        <button type="submit" class="submit-btn" ?disabled=${this.isLoading}>
          ${this.isLoading ? '登录中...' : '登录'}
        </button>
      </form>
    `
  }

  renderWeChatForm() {
    return html`
      <div class="wechat-section">
        <div class="wechat-qr-placeholder">
          <svg viewBox="0 0 24 24" fill="currentColor">
            <path d="M8.691 2.188C3.891 2.188 0 5.476 0 9.53c0 2.212 1.17 4.203 3.002 5.55a.59.59 0 01.213.665l-.39 1.48c-.019.07-.048.141-.048.213 0 .163.13.295.29.295a.326.326 0 00.167-.054l1.903-1.114a.864.864 0 01.717-.098 10.16 10.16 0 002.837.403c.276 0 .543-.027.811-.05-.857-2.578.157-4.972 1.932-6.446 1.703-1.415 3.882-1.98 5.853-1.838-.576-3.583-4.196-6.348-8.596-6.348zM5.785 5.991c.642 0 1.162.529 1.162 1.18a1.17 1.17 0 01-1.162 1.178A1.17 1.17 0 014.623 7.17c0-.651.52-1.18 1.162-1.18zm5.813 0c.642 0 1.162.529 1.162 1.18a1.17 1.17 0 01-1.162 1.178 1.17 1.17 0 01-1.162-1.178c0-.651.52-1.18 1.162-1.18zm5.34 2.867c-1.797-.052-3.746.512-5.28 1.786-1.72 1.428-2.687 3.72-1.78 6.22.942 2.453 3.666 4.229 6.884 4.229.826 0 1.622-.12 2.361-.336a.722.722 0 01.598.082l1.584.926a.272.272 0 00.14.047c.134 0 .24-.111.24-.247 0-.06-.023-.12-.038-.177l-.327-1.233a.582.582 0 01-.023-.156.49.49 0 01.201-.398C23.024 18.48 24 16.82 24 14.98c0-3.21-2.931-5.837-6.656-6.088V8.89a4.78 4.78 0 01.594-.032zm-2.32 1.072c.534 0 .969.44.969.982a.976.976 0 01-.969.983.976.976 0 01-.969-.983c0-.542.435-.982.97-.982zm4.844 0c.534 0 .969.44.969.982a.976.976 0 01-.969.983.976.976 0 01-.969-.983c0-.542.435-.982.969-.982z"/>
          </svg>
          <span>微信扫码登录区域</span>
        </div>
        <p class="wechat-hint">打开微信扫一扫登录</p>
        <button type="button" class="submit-btn" style="margin-top: 16px;" @click=${this.handleWeChatLogin}>
          打开微信扫码
        </button>
      </div>
    `
  }

  render() {
    return html`
      <!-- Left branding side -->
      <div class="brand-side">
        <div class="orb orb-1"></div>
        <div class="orb orb-2"></div>
        <div class="brand-content">
          <div class="brand-logo">
            <logo-icon size="48px"></logo-icon>
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
            <logo-icon size="48px" class="form-logo"></logo-icon>
            <h2 class="form-title">欢迎回来</h2>
            <p class="form-subtitle">请登录您的账户以继续</p>
          </div>

          <div class="form-card">
            <!-- Login method tabs -->
            <div class="login-methods">
              <button
                class="login-method-tab ${this.loginMethod === 'account' ? 'active' : ''}"
                @click=${() => this.setLoginMethod('account')}
              >
                账号登录
              </button>
              <button
                class="login-method-tab ${this.loginMethod === 'phone' ? 'active' : ''}"
                @click=${() => this.setLoginMethod('phone')}
              >
                手机验证码
              </button>
              <button
                class="login-method-tab ${this.loginMethod === 'wechat' ? 'active' : ''}"
                @click=${() => this.setLoginMethod('wechat')}
              >
                微信登录
              </button>
            </div>

            ${this.error ? html`<div class="error-message">${this.error}</div>` : ''}
            ${this.success ? html`<div class="success-message">${this.success}</div>` : ''}

            ${this.loginMethod === 'account' ? this.renderAccountForm() : ''}
            ${this.loginMethod === 'phone' ? this.renderPhoneForm() : ''}
            ${this.loginMethod === 'wechat' ? this.renderWeChatForm() : ''}

          </div>

          <a href="/" class="back-link">← 返回首页</a>
        </div>
      </div>
    `
  }
}

declare global {
  interface HTMLElementTagNameMap {
    'signin-page': SigninPage
  }
}
