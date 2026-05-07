import { LitElement, html } from "lit";
import { customElement, state } from "lit/decorators.js";
import { authApi } from "../../api/auth.js";
import { success } from "../components/toast.js";

const PHONE_RE = /^1[3-9]\d{9}$/;
const USERNAME_RE = /^[a-zA-Z0-9_]{3,32}$/;
const EMAIL_RE = /^[^\s@]+@[^\s@]+\.[^\s@]+$/;

@customElement("view-register")
export class RegisterView extends LitElement {
  @state() loading = false;
  @state() error = "";
  @state() registerSuccess = false;

  @state() username = "";
  @state() phone = "";
  @state() email = "";
  @state() password = "";
  @state() confirmPassword = "";

  @state() countdown = 3;

  private redirectTimer: number | null = null;
  private countdownTimer: number | null = null;

  createRenderRoot() {
    return this;
  }

  disconnectedCallback() {
    super.disconnectedCallback();
    if (this.redirectTimer) clearTimeout(this.redirectTimer);
    if (this.countdownTimer) clearInterval(this.countdownTimer);
  }

  private get strengthHint(): string {
    const p = this.password;
    if (!p) return "";
    if (p.length < 8 || /\s/.test(p)) return "弱";
    const hasLetter = /[A-Za-z]/.test(p);
    const hasDigit = /\d/.test(p);
    if (!hasLetter || !hasDigit) return "弱";
    if (p.length >= 12) return "强";
    return "中";
  }

  private validate(): string {
    const username = this.username.trim();
    const phone = this.phone.trim();
    const email = this.email.trim();

    if (!username) return "请输入用户名";
    if (!USERNAME_RE.test(username)) return "用户名 3-32 字符，仅限字母、数字、下划线";

    if (!phone) return "请输入手机号";
    if (!PHONE_RE.test(phone)) return "请输入正确的中国大陆手机号";

    if (email && !EMAIL_RE.test(email)) return "邮箱格式不正确";

    if (!this.password) return "请输入密码";
    if (this.password.length < 8) return "密码至少 8 个字符";
    if (/\s/.test(this.password)) return "密码不能包含空格";
    if (!/[A-Za-z]/.test(this.password)) return "密码必须包含字母";
    if (!/\d/.test(this.password)) return "密码必须包含数字";

    if (!this.confirmPassword) return "请再次输入密码";
    if (this.password !== this.confirmPassword) return "两次输入的密码不一致";

    return "";
  }

  private mapBackendError(msg: string): string {
    const m = msg || "";
    if (m.includes("用户名已存在")) return "该用户名已被占用";
    if (m.includes("手机号已注册")) return "该手机号已注册";
    if (m.includes("邮箱已注册")) return "该邮箱已注册";
    return m || "注册失败，请稍后重试";
  }

  async handleSubmit(e: Event) {
    e.preventDefault();
    this.error = "";

    const validationError = this.validate();
    if (validationError) {
      this.error = validationError;
      return;
    }

    this.loading = true;
    try {
      const username = this.username.trim();
      const phone = this.phone.trim();
      const email = this.email.trim();
      const res = await authApi.register({
        username,
        phone,
        password: this.password,
        ...(email ? { email } : {}),
        displayName: username,
      });

      const token = res.result?.accessToken;
      this.registerSuccess = true;
      this.countdown = 3;
      success("注册成功");

      this.countdownTimer = window.setInterval(() => {
        this.countdown--;
        if (this.countdown <= 0 && this.countdownTimer) {
          clearInterval(this.countdownTimer);
          this.countdownTimer = null;
        }
      }, 1000);

      if (token) {
        localStorage.setItem("auth-token", token);
        sessionStorage.setItem("auth-token", token);
        const wsId = res.result?.workspaceId;
        if (wsId) {
          localStorage.setItem("workspace-id", wsId);
          sessionStorage.setItem("workspace-id", wsId);
        }
        this.redirectTimer = window.setTimeout(() => {
          window.location.href = "/dashboard";
        }, 3000);
      } else {
        this.redirectTimer = window.setTimeout(() => {
          window.history.pushState({}, "", "/login");
          window.dispatchEvent(new PopStateEvent("popstate"));
        }, 3000);
      }
    } catch (err: any) {
      this.error = this.mapBackendError(err?.message || "");
    } finally {
      this.loading = false;
    }
  }

  goToLogin(e: Event) {
    e.preventDefault();
    window.history.pushState({}, "", "/login");
    window.dispatchEvent(new PopStateEvent("popstate"));
  }

  render() {
    if (this.registerSuccess) {
      return html`
        <div class="register-page-wrapper">
          <div class="container">
            <!-- Left — success branding -->
            <div class="branding-side">
              <div class="brand-content">
                <div class="success-glyph">
                  <svg viewBox="0 0 64 64" fill="none" stroke="currentColor" stroke-width="2">
                    <circle cx="32" cy="32" r="28" />
                    <path d="M20 34l8 8 16-16" stroke-linecap="round" stroke-linejoin="round" />
                  </svg>
                </div>
                <h1 class="brand-headline">欢迎加入<br />TinyIoTHub</h1>
                <p class="brand-subheadline">您的物联网平台已就绪，即刻开始接入设备、配置规则与监控数据。</p>
              </div>
            </div>

            <!-- Right — celebration panel -->
            <div class="form-side">
              <div class="form-container">
                <div class="success-panel">
                  <div class="success-icon">
                    <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5">
                      <path stroke-linecap="round" stroke-linejoin="round" d="M5 13l4 4L19 7" />
                    </svg>
                  </div>
                  <h2 class="form-title">注册成功</h2>
                  <p class="form-subtitle">正在为您创建工作空间，即将跳转…</p>
                  <div class="success-countdown">${this.countdown}s 后自动跳转</div>
                </div>
              </div>
            </div>
          </div>
        </div>
      `;
    }

    const hint = this.strengthHint;

    return html`
      <div class="register-page-wrapper">
        <div class="container">
          <!-- Branding Side -->
          <div class="branding-side">
            <div class="brand-content">
              <div class="brand-logo">
                <img src="/logo.svg" alt="TinyIoTHub" style="width: 48px; height: 48px;" />
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
                  企业级安全，JWT 认证与多租户隔离
                </li>
              </ul>
            </div>
          </div>

          <!-- Form Side -->
          <div class="form-side">
            <div class="form-container">
              <div class="form-header">
                <img src="/logo.svg" alt="TinyIoTHub" style="width: 48px; height: 48px; margin: 0 auto 16px; display: block;" />
                <h2 class="form-title">创建账户</h2>
                <p class="form-subtitle">
                  已有账户？<a href="/login" @click=${this.goToLogin}>立即登录</a>
                </p>
              </div>

              ${this.error ? html`<div class="alert error">${this.error}</div>` : ""}

              <form class="form" novalidate @submit=${this.handleSubmit}>
                <div class="form-group">
                  <label class="form-label" for="reg-username">用户名</label>
                  <input
                    type="text"
                    id="reg-username"
                    class="form-input"
                    autocomplete="username"
                    minlength="3"
                    maxlength="32"
                    placeholder="3-32 字符，字母、数字、下划线"
                    .value=${this.username}
                    @input=${(e: any) => { this.username = e.target.value; }}
                  />
                </div>

                <div class="form-group">
                  <label class="form-label" for="reg-phone">手机号</label>
                  <input
                    type="tel"
                    id="reg-phone"
                    class="form-input"
                    inputmode="numeric"
                    autocomplete="tel-national"
                    maxlength="11"
                    placeholder="11 位中国大陆手机号"
                    .value=${this.phone}
                    @input=${(e: any) => { this.phone = e.target.value.replace(/\D/g, ""); }}
                  />
                </div>

                <div class="form-group">
                  <label class="form-label" for="reg-email">
                    邮箱
                    <span style="color: var(--muted); font-weight: 400; margin-left: 6px;">(选填)</span>
                  </label>
                  <input
                    type="email"
                    id="reg-email"
                    class="form-input"
                    autocomplete="email"
                    placeholder="name@company.com"
                    .value=${this.email}
                    @input=${(e: any) => { this.email = e.target.value; }}
                  />
                </div>

                <div class="form-row">
                  <div class="form-group">
                    <label class="form-label" for="reg-password">
                      密码
                      ${hint ? html`<span style="color: var(--muted); font-weight: 400; margin-left: 6px;">强度 · ${hint}</span>` : ""}
                    </label>
                    <input
                      type="password"
                      id="reg-password"
                      class="form-input"
                      autocomplete="new-password"
                      minlength="8"
                      placeholder="≥ 8 位 · 字母 + 数字"
                      .value=${this.password}
                      @input=${(e: any) => { this.password = e.target.value; }}
                    />
                  </div>
                  <div class="form-group">
                    <label class="form-label" for="reg-confirm">确认密码</label>
                    <input
                      type="password"
                      id="reg-confirm"
                      class="form-input"
                      autocomplete="new-password"
                      placeholder="再次输入密码"
                      .value=${this.confirmPassword}
                      @input=${(e: any) => { this.confirmPassword = e.target.value; }}
                    />
                  </div>
                </div>

                <button type="submit" class="submit-btn" ?disabled=${this.loading}>
                  ${this.loading ? "注册中..." : "立即注册"}
                </button>

                <p class="terms">
                  注册即表示您同意我们的
                  <a href="/terms" target="_blank" rel="noopener">服务条款</a>
                  与
                  <a href="/privacy" target="_blank" rel="noopener">隐私政策</a>
                </p>
              </form>
            </div>
          </div>
        </div>
      </div>
    `;
  }
}
