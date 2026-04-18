import { LitElement, html, nothing } from "lit";
import { customElement, state } from "lit/decorators.js";
import { authApi } from "../../api/auth.js";
import { success } from "../components/toast.js";

type LoginMethod = "account" | "phone" | "wechat";

@customElement("view-login")
export class LoginView extends LitElement {
  @state() method: LoginMethod = "account";
  @state() loading = false;
  @state() error = "";

  // account
  @state() username = "";
  @state() password = "";

  // sms
  @state() phone = "";
  @state() smsCode = "";
  @state() smsCountdown = 0;
  private smsTimer: number | null = null;

  // wechat
  @state() wechatQrUrl = "";
  @state() wechatState = "";
  @state() wechatLoading = false;

  createRenderRoot() {
    return this;
  }

  disconnectedCallback() {
    super.disconnectedCallback();
    if (this.smsTimer) clearInterval(this.smsTimer);
  }

  switchMethod(m: LoginMethod) {
    this.method = m;
    this.error = "";
    if (m === "wechat") {
      this.loadWechatQrcode();
    }
  }

  // ── Account ──

  async handleLogin(e: Event) {
    e.preventDefault();
    if (!this.username.trim() || !this.password.trim()) {
      this.error = "请输入用户名和密码";
      return;
    }
    this.loading = true;
    this.error = "";
    try {
      const res = await authApi.login({ username: this.username, password: this.password });
      const token = res.result?.accessToken;
      if (!token) throw new Error("登录失败：未获取到令牌");
      this.onLoginSuccess(token, res.result?.workspaceId);
    } catch (err: any) {
      this.error = err.message || "登录失败";
    } finally {
      this.loading = false;
    }
  }

  // ── SMS ──

  async sendSmsCode() {
    if (!this.phone.trim()) {
      this.error = "请输入手机号";
      return;
    }
    if (this.smsCountdown > 0) return;
    this.error = "";
    try {
      const res = await authApi.smsSend({ phone: this.phone.trim() });
      const expire = res.result?.expiresIn || 60;
      success(`验证码已发送，有效期 ${expire} 秒`);
      this.smsCountdown = expire;
      this.smsTimer = window.setInterval(() => {
        this.smsCountdown--;
        if (this.smsCountdown <= 0 && this.smsTimer) {
          clearInterval(this.smsTimer);
          this.smsTimer = null;
        }
      }, 1000);
    } catch (err: any) {
      this.error = err.message || "发送验证码失败";
    }
  }

  async handleSmsLogin(e: Event) {
    e.preventDefault();
    if (!this.phone.trim() || !this.smsCode.trim()) {
      this.error = "请输入手机号和验证码";
      return;
    }
    this.loading = true;
    this.error = "";
    try {
      const res = await authApi.smsLogin({ phone: this.phone.trim(), code: this.smsCode.trim() });
      const token = res.result?.accessToken;
      if (!token) throw new Error("登录失败：未获取到令牌");
      this.onLoginSuccess(token, res.result?.workspaceId);
    } catch (err: any) {
      this.error = err.message || "短信登录失败";
    } finally {
      this.loading = false;
    }
  }

  // ── WeChat ──

  async loadWechatQrcode() {
    this.wechatLoading = true;
    this.error = "";
    try {
      const res = await authApi.getWechatQrcode();
      const data = res.result;
      if (!data) throw new Error("获取二维码失败");
      this.wechatQrUrl = data.qrcodeUrl;
      this.wechatState = data.state;
    } catch (err: any) {
      this.error = err.message || "获取微信二维码失败";
    } finally {
      this.wechatLoading = false;
    }
  }

  // ── Common ──

  onLoginSuccess(token: string, workspaceId?: string) {
    localStorage.setItem("auth-token", token);
    sessionStorage.setItem("auth-token", token);
    if (workspaceId) {
      localStorage.setItem("workspace-id", workspaceId);
      sessionStorage.setItem("workspace-id", workspaceId);
    }
    window.location.href = "/dashboard";
  }

  goToRegister(e: Event) {
    e.preventDefault();
    window.history.pushState({}, "", "/register");
    window.dispatchEvent(new PopStateEvent("popstate"));
  }

  goHome(e: Event) {
    e.preventDefault();
    window.history.pushState({}, "", "/home");
    window.dispatchEvent(new PopStateEvent("popstate"));
  }

  // ── Render ──

  render() {
    return html`
      <div class="signin-page-wrapper">
        <!-- Left branding side -->
        <div class="brand-side">
          <div class="orb orb-1"></div>
          <div class="orb orb-2"></div>
          <div class="brand-content">
            <div class="brand-logo">
              <img src="/logo.svg" alt="TinyIoTHub" style="width: 48px; height: 48px;" />
              <h1>TinyIoTHub</h1>
            </div>
            <h2 class="brand-headline">云端 SaaS<br />物联网平台</h2>
            <p class="brand-tagline">
              支持配置和管理边缘网关设备，兼容多协议。在云端统一管理 Modbus、ONVIF、SNMP 与 MQTT 设备，实时监控和数据采集。
            </p>
            <div class="brand-stats">
              <div class="brand-stat">
                <div class="brand-stat__num">Modbus</div>
                <div class="brand-stat__label">工业协议</div>
              </div>
              <div class="brand-stat">
                <div class="brand-stat__num">MQTT</div>
                <div class="brand-stat__label">消息协议</div>
              </div>
              <div class="brand-stat">
                <div class="brand-stat__num">ONVIF</div>
                <div class="brand-stat__label">视频协议</div>
              </div>
            </div>
          </div>
        </div>

        <!-- Right form side -->
        <div class="form-side">
          <div class="form-container">
            <div class="form-header">
              <img src="/logo.svg" alt="TinyIoTHub" class="form-logo" />
              <h2 class="form-title">欢迎回来</h2>
              <p class="form-subtitle">请登录您的账户以继续</p>
            </div>

            <div class="form-card">
              <!-- Login method tabs -->
              <div class="login-methods">
                <button
                  class="login-method-tab ${this.method === "account" ? "active" : ""}"
                  @click=${() => this.switchMethod("account")}
                >账号登录</button>
                <button
                  class="login-method-tab ${this.method === "phone" ? "active" : ""}"
                  @click=${() => this.switchMethod("phone")}
                >手机验证码</button>
                <button
                  class="login-method-tab ${this.method === "wechat" ? "active" : ""}"
                  @click=${() => this.switchMethod("wechat")}
                >微信登录</button>
              </div>

              ${this.error ? html`<div class="error-message">${this.error}</div>` : ""}

              ${this.method === "account" ? this.renderAccountTab() : nothing}
              ${this.method === "phone" ? this.renderPhoneTab() : nothing}
              ${this.method === "wechat" ? this.renderWechatTab() : nothing}

              <div style="text-align: center; margin-top: 16px;">
                <span style="font-size: 13px; color: var(--muted);">还没有账户？</span>
                <a href="/register" @click=${this.goToRegister} style="font-size: 13px; color: var(--accent); text-decoration: none; font-weight: 500;">立即注册</a>
              </div>
            </div>

            <a href="/home" class="back-link" @click=${this.goHome}>← 返回首页</a>
          </div>
        </div>
      </div>
    `;
  }

  renderAccountTab() {
    return html`
      <form @submit=${this.handleLogin}>
        <div class="form-group">
          <label class="form-label" for="username">用户名</label>
          <input
            type="text"
            id="username"
            class="form-input"
            placeholder="请输入用户名"
            .value=${this.username}
            @input=${(e: any) => { this.username = e.target.value; }}
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
            @input=${(e: any) => { this.password = e.target.value; }}
            required
          />
        </div>
        <div class="form-group">
        </div>
        <button type="submit" class="submit-btn" ?disabled=${this.loading}>
          ${this.loading ? "登录中..." : "登录"}
        </button>
      </form>
    `;
  }

  renderPhoneTab() {
    return html`
      <form @submit=${this.handleSmsLogin}>
        <div class="form-group">
          <label class="form-label" for="phone">手机号</label>
          <input
            type="tel"
            id="phone"
            class="form-input"
            placeholder="请输入手机号"
            .value=${this.phone}
            @input=${(e: any) => { this.phone = e.target.value; }}
            required
          />
        </div>
        <div class="form-group">
          <label class="form-label" for="smsCode">验证码</label>
          <div class="phone-input-group">
            <input
              type="text"
              id="smsCode"
              class="form-input"
              placeholder="请输入验证码"
              .value=${this.smsCode}
              @input=${(e: any) => { this.smsCode = e.target.value; }}
              required
            />
            <button
              type="button"
              class="send-code-btn"
              ?disabled=${this.smsCountdown > 0}
              @click=${this.sendSmsCode}
            >
              ${this.smsCountdown > 0 ? `${this.smsCountdown}s` : "发送验证码"}
            </button>
          </div>
        </div>
        <div class="form-group">
        </div>
        <button type="submit" class="submit-btn" ?disabled=${this.loading}>
          ${this.loading ? "登录中..." : "登录"}
        </button>
      </form>
    `;
  }

  renderWechatTab() {
    return html`
      <div class="wechat-section">
        ${this.wechatLoading ? html`
          <div class="wechat-qr-placeholder">
            <span class="loading-spinner"></span>
            <span>加载二维码...</span>
          </div>
        ` : this.wechatQrUrl ? html`
          <div class="wechat-qr-area">
            <img src=${this.wechatQrUrl} alt="微信登录二维码" />
            <p class="wechat-hint">请使用微信扫描二维码登录</p>
          </div>
        ` : html`
          <div class="wechat-qr-placeholder">
            <svg viewBox="0 0 24 24" fill="currentColor">
              <path d="M8.691 2.188C3.891 2.188 0 5.476 0 9.53c0 2.212 1.17 4.203 3.002 5.55a.59.59 0 01.213.665l-.39 1.48c-.019.07-.048.141-.048.213 0 .163.13.295.29.295a.326.326 0 00.167-.054l1.903-1.114a.864.864 0 01.717-.098 10.16 10.16 0 002.837.403c.276 0 .543-.027.811-.05-.857-2.578.157-4.972 1.932-6.446 1.703-1.415 3.882-1.98 5.853-1.838-.576-3.583-4.196-6.348-8.596-6.348z"/>
            </svg>
            <span>二维码加载失败</span>
          </div>
          <button type="button" class="send-code-btn" style="margin-top: 12px;" @click=${() => this.loadWechatQrcode()}>刷新二维码</button>
        `}
      </div>
    `;
  }
}
