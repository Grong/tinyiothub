import { LitElement, html } from "lit";
import { customElement, state } from "lit/decorators.js";
import { apiPost } from "../../api/client.js";
import { success } from "../components/toast.js";

@customElement("view-register")
export class RegisterView extends LitElement {
  @state() loading = false;
  @state() error = "";
  @state() registerSuccess = false;

  @state() name = "";
  @state() username = "";
  @state() email = "";
  @state() password = "";
  @state() confirmPassword = "";

  private redirectTimer: number | null = null;

  createRenderRoot() {
    return this;
  }

  disconnectedCallback() {
    super.disconnectedCallback();
    if (this.redirectTimer) clearTimeout(this.redirectTimer);
  }

  async handleSubmit(e: Event) {
    e.preventDefault();
    this.error = "";

    if (!this.name.trim()) { this.error = "请输入姓名"; return; }
    if (!this.username.trim()) { this.error = "请输入用户名"; return; }
    if (this.username.trim().length < 3) { this.error = "用户名至少3个字符"; return; }
    if (!this.email.trim()) { this.error = "请输入邮箱"; return; }
    if (!/^[^\s@]+@[^\s@]+\.[^\s@]+$/.test(this.email.trim())) { this.error = "请输入有效的邮箱地址"; return; }
    if (this.password.length < 6) { this.error = "密码至少6个字符"; return; }
    if (this.password !== this.confirmPassword) { this.error = "两次输入的密码不一致"; return; }

    this.loading = true;
    try {
      await apiPost("/users", {
        name: this.name.trim(),
        username: this.username.trim(),
        password: this.password,
        email: this.email.trim(),
      });
      this.registerSuccess = true;
      success("注册成功！");
      this.redirectTimer = window.setTimeout(() => {
        window.history.pushState({}, "", "/login");
        window.dispatchEvent(new PopStateEvent("popstate"));
      }, 2000);
    } catch (err: any) {
      this.error = err.message || "注册失败，请稍后重试";
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
            <div class="form-side">
              <div class="form-container">
                <div class="alert success">
                  注册成功！正在跳转到登录页面...
                </div>
              </div>
            </div>
          </div>
        </div>
      `;
    }

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
                <img src="/logo.svg" alt="TinyIoTHub" style="width: 48px; height: 48px; margin: 0 auto 16px; display: block;" />
                <h2 class="form-title">创建账户</h2>
                <p class="form-subtitle">
                  已有账户？<a href="/login" @click=${this.goToLogin}>立即登录</a>
                </p>
              </div>

              ${this.error ? html`<div class="alert error">${this.error}</div>` : ""}

              <form class="form" @submit=${this.handleSubmit}>
                <div class="form-group">
                  <label class="form-label" for="reg-name">姓名</label>
                  <input
                    type="text"
                    id="reg-name"
                    class="form-input"
                    placeholder="请输入姓名"
                    .value=${this.name}
                    @input=${(e: any) => { this.name = e.target.value; }}
                  />
                </div>

                <div class="form-group">
                  <label class="form-label" for="reg-username">用户名</label>
                  <input
                    type="text"
                    id="reg-username"
                    class="form-input"
                    placeholder="请输入用户名（至少3个字符）"
                    .value=${this.username}
                    @input=${(e: any) => { this.username = e.target.value; }}
                  />
                </div>

                <div class="form-group">
                  <label class="form-label" for="reg-email">邮箱</label>
                  <input
                    type="email"
                    id="reg-email"
                    class="form-input"
                    placeholder="name@company.com"
                    .value=${this.email}
                    @input=${(e: any) => { this.email = e.target.value; }}
                  />
                </div>

                <div class="form-row">
                  <div class="form-group">
                    <label class="form-label" for="reg-password">密码</label>
                    <input
                      type="password"
                      id="reg-password"
                      class="form-input"
                      placeholder="至少6个字符"
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
                      placeholder="再次输入密码"
                      .value=${this.confirmPassword}
                      @input=${(e: any) => { this.confirmPassword = e.target.value; }}
                    />
                  </div>
                </div>

                <button type="submit" class="submit-btn" ?disabled=${this.loading}>
                  ${this.loading ? "注册中..." : "创建账户"}
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
      </div>
    `;
  }
}
