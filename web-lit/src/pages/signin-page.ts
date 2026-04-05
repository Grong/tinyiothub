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
            <svg viewBox="0 0 56 56" style="width:48px;height:48px;color:var(--accent);">
              <path d="M1890 7776c-165 -46 -338 -179 -462 -356 -217 -309 -439 -926 -572 -1590 -72 -358 -110 -663 -132 -1050 l-7 -124 -45 -61c-208 -282 -364 -669 -418 -1040 -25 -167 -25 -505 0 -670 31 -208 106 -435 198 -605 75 -137 206 -320 230 -320 6 0 27 14 48 30 63 50 199 117 295 146 123 36 320 44 462 19 269 -48 489 -145 690 -303 292 -230 483 -600 483 -933 l0 -85 163 -32c342 -68 565 -87 1007 -87 445 0 613 14 1021 85 l126 23 6 126c24 591 511 1096 1165 1207 138 24 326 15 447 -21 113 -33 176 -62 255 -115 34 -23 63 -40 64 -38 85 104 191 279 254 424 115 261 164 508 164 819 0 301 -49 564 -160 855 -76 201 -223 461 -358 634 -39 49 -54 77 -54 100 0 147 -112 787 -196 1120 -173 688 -448 1316 -688 1574 -114 122 -287 234 -413 267 -129 34 -292 11 -395 -54 -103 -65 -163 -141 -323 -406 -118 -196 -294 -575 -395 -850 -18 -49 -36 -93 -39 -96 -3 -4 -40 -1 -83 7 -145 25 -284 35 -513 34 -211 0 -384 -13 -610 -46 l-40 -6 -39 99c-22 54 -76 181 -121 283 -249 562 -467 901 -641 992 -103 55 -265 74 -374 44z m2173 -3657c197 -24 459 -95 503 -135 34 -33 32 -77 -6 -116 -36 -36 -58 -35 -194 6 -381 114 -807 114 -1183 -1 -116 -36 -158 -35 -189 1 -54 62 -13 120 112 159 43 14 131 36 194 50 100 23 169 32 355 51 60 6 315 -3 408 -15z m-1842 -263c42 -38 146 -76 209 -76 56 0 87 -15 110 -52 61 -100 -30 -201 -230 -255 -90 -25 -309 -24 -399 1 -153 42 -241 114 -241 197 0 115 198 216 429 218 82 1 84 0 122 -33z m3424 9c85 -22 161 -63 201 -107 29 -33 34 -45 34 -88 0 -43 -5 -55 -35 -89 -152 -168 -633 -173 -797 -8 -42 42 -54 83 -38 131 16 50 38 64 126 80 90 15 161 45 195 83 24 25 25 25 139 20 63 -3 142 -13 175 -22z m-634 -704c34 -34 36 -58 10 -109 -65 -126 -223 -247 -376 -287 -35 -9 -102 -15 -166 -15 -95 0 -127 5 -285 47 l-179 46 -225 1 -225 0 -155 -42c-191 -53 -350 -66 -460 -37 -102 26 -196 81 -277 161 -110 107 -135 187 -75 234 13 11 40 20 59 20 36 0 45 -7 120 -97 126 -152 318 -190 568 -115 319 97 556 94 960 -13 84 -22 116 -26 201 -23 91 3 107 7 167 37 64 33 148 107 187 165 40 62 104 73 151 27z m-1023 -617c92 -19 179 -48 247 -82 120 -60 249 -170 298 -253 l27 -46 -20 -66c-39 -125 -149 -262 -286 -352 -170 -113 -410 -162 -613 -125 -291 53 -551 266 -613 502 -12 46 -11 48 24 101 99 149 341 289 561 326 90 15 291 12 375 -5z m-1562 -1589c-278 -50 -513 -295 -614 -641 -56 -194 -90 -441 -73 -534 20 -107 87 -186 187 -219 45 -15 190 -16 252 -2 l41 9 54 -57c33 -34 78 -68 113 -86 53 -26 71 -29 150 -29 70 0 104 5 152 24 34 13 67 29 74 37 9 11 19 7 53 -21 112 -92 282 -112 410 -48 43 22 114 91 142 138 24 42 25 42 93 48 208 17 310 160 281 394 -36 291 -177 541 -404 717 -191 149 -397 237 -631 271 -122 18 -176 18 -280 -1z m5095 5c-289 -33 -575 -172 -768 -373 -156 -164 -250 -352 -288 -577 -35 -214 5 -338 132 -400 46 -23 80 -32 124 -34 60 -2 61 -3 81 -41 32 -60 88 -112 159 -147 57 -28 74 -32 145 -31 89 0 166 28 238 86 l38 30 76 -38c72 -37 81 -39 167 -39 83 0 98 3 158 33 46 22 83 51 117 88 l49 56 70 -12c96 -17 226 -9 277 17 50 26 96 75 126 133 22 41 24 57 23 175 -2 257 -91 572 -214 749 -87 127 -223 238 -348 283 -59 22 -234 55 -272 52 -8 -1 -49 -5 -90 -10z" fill="currentColor"/>
            </svg>
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
            <svg class="form-logo" viewBox="0 0 56 56" style="width:48px;height:48px;color:var(--accent);">
              <path d="M1890 7776c-165 -46 -338 -179 -462 -356 -217 -309 -439 -926 -572 -1590 -72 -358 -110 -663 -132 -1050 l-7 -124 -45 -61c-208 -282 -364 -669 -418 -1040 -25 -167 -25 -505 0 -670 31 -208 106 -435 198 -605 75 -137 206 -320 230 -320 6 0 27 14 48 30 63 50 199 117 295 146 123 36 320 44 462 19 269 -48 489 -145 690 -303 292 -230 483 -600 483 -933 l0 -85 163 -32c342 -68 565 -87 1007 -87 445 0 613 14 1021 85 l126 23 6 126c24 591 511 1096 1165 1207 138 24 326 15 447 -21 113 -33 176 -62 255 -115 34 -23 63 -40 64 -38 85 104 191 279 254 424 115 261 164 508 164 819 0 301 -49 564 -160 855 -76 201 -223 461 -358 634 -39 49 -54 77 -54 100 0 147 -112 787 -196 1120 -173 688 -448 1316 -688 1574 -114 122 -287 234 -413 267 -129 34 -292 11 -395 -54 -103 -65 -163 -141 -323 -406 -118 -196 -294 -575 -395 -850 -18 -49 -36 -93 -39 -96 -3 -4 -40 -1 -83 7 -145 25 -284 35 -513 34 -211 0 -384 -13 -610 -46 l-40 -6 -39 99c-22 54 -76 181 -121 283 -249 562 -467 901 -641 992 -103 55 -265 74 -374 44z m2173 -3657c197 -24 459 -95 503 -135 34 -33 32 -77 -6 -116 -36 -36 -58 -35 -194 6 -381 114 -807 114 -1183 -1 -116 -36 -158 -35 -189 1 -54 62 -13 120 112 159 43 14 131 36 194 50 100 23 169 32 355 51 60 6 315 -3 408 -15z m-1842 -263c42 -38 146 -76 209 -76 56 0 87 -15 110 -52 61 -100 -30 -201 -230 -255 -90 -25 -309 -24 -399 1 -153 42 -241 114 -241 197 0 115 198 216 429 218 82 1 84 0 122 -33z m3424 9c85 -22 161 -63 201 -107 29 -33 34 -45 34 -88 0 -43 -5 -55 -35 -89 -152 -168 -633 -173 -797 -8 -42 42 -54 83 -38 131 16 50 38 64 126 80 90 15 161 45 195 83 24 25 25 25 139 20 63 -3 142 -13 175 -22z m-634 -704c34 -34 36 -58 10 -109 -65 -126 -223 -247 -376 -287 -35 -9 -102 -15 -166 -15 -95 0 -127 5 -285 47 l-179 46 -225 1 -225 0 -155 -42c-191 -53 -350 -66 -460 -37 -102 26 -196 81 -277 161 -110 107 -135 187 -75 234 13 11 40 20 59 20 36 0 45 -7 120 -97 126 -152 318 -190 568 -115 319 97 556 94 960 -13 84 -22 116 -26 201 -23 91 3 107 7 167 37 64 33 148 107 187 165 40 62 104 73 151 27z m-1023 -617c92 -19 179 -48 247 -82 120 -60 249 -170 298 -253 l27 -46 -20 -66c-39 -125 -149 -262 -286 -352 -170 -113 -410 -162 -613 -125 -291 53 -551 266 -613 502 -12 46 -11 48 24 101 99 149 341 289 561 326 90 15 291 12 375 -5z m-1562 -1589c-278 -50 -513 -295 -614 -641 -56 -194 -90 -441 -73 -534 20 -107 87 -186 187 -219 45 -15 190 -16 252 -2 l41 9 54 -57c33 -34 78 -68 113 -86 53 -26 71 -29 150 -29 70 0 104 5 152 24 34 13 67 29 74 37 9 11 19 7 53 -21 112 -92 282 -112 410 -48 43 22 114 91 142 138 24 42 25 42 93 48 208 17 310 160 281 394 -36 291 -177 541 -404 717 -191 149 -397 237 -631 271 -122 18 -176 18 -280 -1z m5095 5c-289 -33 -575 -172 -768 -373 -156 -164 -250 -352 -288 -577 -35 -214 5 -338 132 -400 46 -23 80 -32 124 -34 60 -2 61 -3 81 -41 32 -60 88 -112 159 -147 57 -28 74 -32 145 -31 89 0 166 28 238 86 l38 30 76 -38c72 -37 81 -39 167 -39 83 0 98 3 158 33 46 22 83 51 117 88 l49 56 70 -12c96 -17 226 -9 277 17 50 26 96 75 126 133 22 41 24 57 23 175 -2 257 -91 572 -214 749 -87 127 -223 238 -348 283 -59 22 -234 55 -272 52 -8 -1 -49 -5 -90 -10z" fill="currentColor"/>
            </svg>
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
