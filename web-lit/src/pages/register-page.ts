import { LitElement, html, css } from 'lit'
import { customElement, state } from 'lit/decorators.js'
import { tenantApi } from '../services/tenant'
import { navigate } from '../lib/navigate'

@customElement('register-page')
export class RegisterPage extends LitElement {
  static styles = css`
    :host {
      position: fixed;
      inset: 0;
      z-index: 100;
      display: block;
      min-height: 100vh;
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
              <svg viewBox="0 0 56 56" style="width:48px;height:48px;color:var(--accent);">
                <path d="M1890 7776c-165 -46 -338 -179 -462 -356 -217 -309 -439 -926 -572 -1590 -72 -358 -110 -663 -132 -1050 l-7 -124 -45 -61c-208 -282 -364 -669 -418 -1040 -25 -167 -25 -505 0 -670 31 -208 106 -435 198 -605 75 -137 206 -320 230 -320 6 0 27 14 48 30 63 50 199 117 295 146 123 36 320 44 462 19 269 -48 489 -145 690 -303 292 -230 483 -600 483 -933 l0 -85 163 -32c342 -68 565 -87 1007 -87 445 0 613 14 1021 85 l126 23 6 126c24 591 511 1096 1165 1207 138 24 326 15 447 -21 113 -33 176 -62 255 -115 34 -23 63 -40 64 -38 85 104 191 279 254 424 115 261 164 508 164 819 0 301 -49 564 -160 855 -76 201 -223 461 -358 634 -39 49 -54 77 -54 100 0 147 -112 787 -196 1120 -173 688 -448 1316 -688 1574 -114 122 -287 234 -413 267 -129 34 -292 11 -395 -54 -103 -65 -163 -141 -323 -406 -118 -196 -294 -575 -395 -850 -18 -49 -36 -93 -39 -96 -3 -4 -40 -1 -83 7 -145 25 -284 35 -513 34 -211 0 -384 -13 -610 -46 l-40 -6 -39 99c-22 54 -76 181 -121 283 -249 562 -467 901 -641 992 -103 55 -265 74 -374 44z m2173 -3657c197 -24 459 -95 503 -135 34 -33 32 -77 -6 -116 -36 -36 -58 -35 -194 6 -381 114 -807 114 -1183 -1 -116 -36 -158 -35 -189 1 -54 62 -13 120 112 159 43 14 131 36 194 50 100 23 169 32 355 51 60 6 315 -3 408 -15z m-1842 -263c42 -38 146 -76 209 -76 56 0 87 -15 110 -52 61 -100 -30 -201 -230 -255 -90 -25 -309 -24 -399 1 -153 42 -241 114 -241 197 0 115 198 216 429 218 82 1 84 0 122 -33z m3424 9c85 -22 161 -63 201 -107 29 -33 34 -45 34 -88 0 -43 -5 -55 -35 -89 -152 -168 -633 -173 -797 -8 -42 42 -54 83 -38 131 16 50 38 64 126 80 90 15 161 45 195 83 24 25 25 25 139 20 63 -3 142 -13 175 -22z m-634 -704c34 -34 36 -58 10 -109 -65 -126 -223 -247 -376 -287 -35 -9 -102 -15 -166 -15 -95 0 -127 5 -285 47 l-179 46 -225 1 -225 0 -155 -42c-191 -53 -350 -66 -460 -37 -102 26 -196 81 -277 161 -110 107 -135 187 -75 234 13 11 40 20 59 20 36 0 45 -7 120 -97 126 -152 318 -190 568 -115 319 97 556 94 960 -13 84 -22 116 -26 201 -23 91 3 107 7 167 37 64 33 148 107 187 165 40 62 104 73 151 27z m-1023 -617c92 -19 179 -48 247 -82 120 -60 249 -170 298 -253 l27 -46 -20 -66c-39 -125 -149 -262 -286 -352 -170 -113 -410 -162 -613 -125 -291 53 -551 266 -613 502 -12 46 -11 48 24 101 99 149 341 289 561 326 90 15 291 12 375 -5z m-1562 -1589c-278 -50 -513 -295 -614 -641 -56 -194 -90 -441 -73 -534 20 -107 87 -186 187 -219 45 -15 190 -16 252 -2 l41 9 54 -57c33 -34 78 -68 113 -86 53 -26 71 -29 150 -29 70 0 104 5 152 24 34 13 67 29 74 37 9 11 19 7 53 -21 112 -92 282 -112 410 -48 43 22 114 91 142 138 24 42 25 42 93 48 208 17 310 160 281 394 -36 291 -177 541 -404 717 -191 149 -397 237 -631 271 -122 18 -176 18 -280 -1z m5095 5c-289 -33 -575 -172 -768 -373 -156 -164 -250 -352 -288 -577 -35 -214 5 -338 132 -400 46 -23 80 -32 124 -34 60 -2 61 -3 81 -41 32 -60 88 -112 159 -147 57 -28 74 -32 145 -31 89 0 166 28 238 86 l38 30 76 -38c72 -37 81 -39 167 -39 83 0 98 3 158 33 46 22 83 51 117 88 l49 56 70 -12c96 -17 226 -9 277 17 50 26 96 75 126 133 22 41 24 57 23 175 -2 257 -91 572 -214 749 -87 127 -223 238 -348 283 -59 22 -234 55 -272 52 -8 -1 -49 -5 -90 -10z" fill="currentColor"/>
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
