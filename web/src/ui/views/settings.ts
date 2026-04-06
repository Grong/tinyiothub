import { LitElement, html, nothing } from "lit";
import { customElement, state } from "lit/decorators.js";
import { authApi } from "../../api/auth.js";
import { userApi } from "../../api/users.js";
import type { UserProfile } from "../../types/index.js";
import { success, error as toastError } from "../components/toast.js";

@customElement("view-settings")
export class SettingsView extends LitElement {
  @state() activeTab = "profile";
  @state() profile: UserProfile | null = null;
  @state() loading = true;
  @state() saving = false;

  // Profile form
  @state() name = "";
  @state() email = "";
  @state() phone = "";

  // Password form
  @state() oldPassword = "";
  @state() newPassword = "";
  @state() confirmPassword = "";

  createRenderRoot() {
    return this;
  }

  connectedCallback() {
    super.connectedCallback();
    this.loadProfile();
  }

  async loadProfile() {
    this.loading = true;
    try {
      const res = await authApi.getCurrentUser();
      this.profile = res.result;
      if (this.profile) {
        this.name = this.profile.name || "";
        this.email = this.profile.email || "";
        this.phone = this.profile.phone || "";
      }
    } catch (err: any) {
      toastError(err.message || "加载用户信息失败");
    } finally {
      this.loading = false;
    }
  }

  async saveProfile() {
    if (!this.profile) return;
    this.saving = true;
    try {
      await userApi.updateUser(this.profile.id, {
        name: this.name,
        email: this.email,
        phone: this.phone,
      });
      success("个人资料已更新");
      await this.loadProfile();
    } catch (err: any) {
      toastError(err.message || "更新失败");
    } finally {
      this.saving = false;
    }
  }

  async changePassword() {
    if (!this.profile) return;
    if (!this.oldPassword || !this.newPassword) {
      toastError("请填写所有密码字段");
      return;
    }
    if (this.newPassword !== this.confirmPassword) {
      toastError("两次输入的新密码不一致");
      return;
    }
    if (this.newPassword.length < 6) {
      toastError("新密码至少 6 位");
      return;
    }
    this.saving = true;
    try {
      await userApi.changePassword(this.profile.id, {
        oldPassword: this.oldPassword,
        newPassword: this.newPassword,
      });
      success("密码已修改");
      this.oldPassword = "";
      this.newPassword = "";
      this.confirmPassword = "";
    } catch (err: any) {
      toastError(err.message || "修改密码失败");
    } finally {
      this.saving = false;
    }
  }

  renderTabBar() {
    const tabs = [
      { key: "profile", label: "个人资料" },
      { key: "security", label: "安全设置" },
    ];
    return html`
      <div class="settings-tabs">
        ${tabs.map(t => html`
          <button
            class="settings-tab ${this.activeTab === t.key ? "active" : ""}"
            @click=${() => (this.activeTab = t.key)}
          >${t.label}</button>
        `)}
      </div>
    `;
  }

  renderProfileTab() {
    const p = this.profile;
    return html`
      <div class="settings-section">
        <div class="settings-avatar-row">
          <div class="settings-avatar">
            ${p?.avatar
              ? html`<img src=${p.avatar} alt="avatar" />`
              : html`<span>${(p?.name || "?")[0].toUpperCase()}</span>`
            }
          </div>
          <div>
            <div style="font-weight: 600; font-size: 16px;">${p?.name || "-"}</div>
            <div style="font-size: 13px; color: var(--muted);">${p?.role || "用户"}</div>
          </div>
        </div>

        <div class="form-group">
          <label class="form-label">用户名</label>
          <input
            class="form-input"
            type="text"
            .value=${this.name}
            @input=${(e: any) => (this.name = e.target.value)}
            placeholder="请输入用户名"
          />
        </div>

        <div class="form-group">
          <label class="form-label">邮箱</label>
          <input
            class="form-input"
            type="email"
            .value=${this.email}
            @input=${(e: any) => (this.email = e.target.value)}
            placeholder="请输入邮箱"
          />
        </div>

        <div class="form-group">
          <label class="form-label">手机号</label>
          <input
            class="form-input"
            type="tel"
            .value=${this.phone}
            @input=${(e: any) => (this.phone = e.target.value)}
            placeholder="请输入手机号"
          />
        </div>

        <button
          class="submit-btn"
          ?disabled=${this.saving}
          @click=${this.saveProfile}
        >${this.saving ? "保存中..." : "保存修改"}</button>
      </div>
    `;
  }

  renderSecurityTab() {
    return html`
      <div class="settings-section">
        <h3 class="settings-section-title">修改密码</h3>

        <div class="form-group">
          <label class="form-label">当前密码</label>
          <input
            class="form-input"
            type="password"
            .value=${this.oldPassword}
            @input=${(e: any) => (this.oldPassword = e.target.value)}
            placeholder="请输入当前密码"
          />
        </div>

        <div class="form-group">
          <label class="form-label">新密码</label>
          <input
            class="form-input"
            type="password"
            .value=${this.newPassword}
            @input=${(e: any) => (this.newPassword = e.target.value)}
            placeholder="请输入新密码（至少 6 位）"
          />
        </div>

        <div class="form-group">
          <label class="form-label">确认新密码</label>
          <input
            class="form-input"
            type="password"
            .value=${this.confirmPassword}
            @input=${(e: any) => (this.confirmPassword = e.target.value)}
            placeholder="请再次输入新密码"
          />
        </div>

        <button
          class="submit-btn"
          ?disabled=${this.saving}
          @click=${this.changePassword}
        >${this.saving ? "修改中..." : "修改密码"}</button>
      </div>
    `;
  }

  render() {
    if (this.loading) {
      return html`
        <div style="display: flex; align-items: center; justify-content: center; padding: 60px;">
          <span class="loading-spinner"></span>
          <span style="margin-left: 8px; color: var(--muted);">加载中...</span>
        </div>
      `;
    }

    return html`
      <div class="settings-page">
        ${this.renderTabBar()}
        <div class="settings-content">
          ${this.activeTab === "profile" ? this.renderProfileTab() : nothing}
          ${this.activeTab === "security" ? this.renderSecurityTab() : nothing}
        </div>
      </div>
    `;
  }
}
