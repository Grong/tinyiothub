import { LitElement, html, css } from 'lit'
import { customElement, state } from 'lit/decorators.js'
import { authApi, type UserProfile } from '../services/auth'
import { $user, type User } from '../stores/auth-store'
import { hostStyles } from '../styles/shared-host'

@customElement('settings-page')
export class SettingsPage extends LitElement {
  static styles = [hostStyles, css`
    settings-page {
      display: flex;
      flex-direction: column;
      padding: 0;
      background: var(--bg);
      flex: 1;
      min-height: 0;
    }

    /* Header */
    .page-header {
      margin-bottom: 24px;
    }

    .page-title {
      font-size: 24px;
      font-weight: 700;
      color: var(--text-strong);
      margin: 0 0 8px;
    }

    .page-subtitle {
      font-size: 14px;
      color: var(--muted);
      margin: 0;
    }

    /* Tabs */
    .tabs {
      display: flex;
      gap: 4px;
      margin-bottom: 24px;
      box-shadow: 0 1px 0 var(--card-highlight);
    }

    .tab {
      padding: 12px 20px;
      border: none;
      background: transparent;
      color: var(--muted);
      font-size: 14px;
      font-weight: 500;
      cursor: pointer;
      border-bottom: 2px solid transparent;
      margin-bottom: -1px;
      transition: color var(--duration-fast) ease, border-color var(--duration-fast) ease;
    }

    .tab:hover {
      color: var(--text);
    }

    .tab.active {
      color: var(--accent);
      border-bottom-color: var(--accent);
    }

    /* Card */
    .card {
      background: var(--card);
      box-shadow: var(--glass-shadow-sm);
      border-radius: var(--radius-lg);
      overflow: hidden;
      margin-bottom: 24px;
    }

    .card-header {
      padding: 16px 20px;
      box-shadow: 0 1px 0 var(--card-highlight);
    }

    .card-title {
      font-size: 15px;
      font-weight: 600;
      color: var(--text-strong);
      margin: 0;
    }

    .card-body {
      padding: 20px;
    }

    /* Form */
    .form-grid {
      display: grid;
      grid-template-columns: repeat(2, 1fr);
      gap: 20px;
    }

    @media (max-width: 768px) {
      .form-grid {
        grid-template-columns: 1fr;
      }
    }

    .form-group {
      display: flex;
      flex-direction: column;
      gap: 8px;
    }

    .form-group.full {
      grid-column: 1 / -1;
    }

    .form-label {
      font-size: 13px;
      font-weight: 500;
      color: var(--text);
    }

    .form-input {
      padding: 10px 14px;
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

    .form-input:disabled {
      opacity: 0.6;
      cursor: not-allowed;
    }

    .form-hint {
      font-size: 12px;
      color: var(--muted);
    }

    /* Buttons */
    .btn-row {
      display: flex;
      justify-content: flex-end;
      gap: 12px;
      padding: 16px 20px;
      box-shadow: 0 -1px 0 var(--card-highlight);
      background: var(--bg);
    }

    .btn {
      display: inline-flex;
      align-items: center;
      gap: 8px;
      padding: 10px 20px;
      box-shadow: var(--glass-shadow-sm);
      border-radius: var(--radius-md);
      background: var(--card);
      color: var(--text);
      font-size: 13px;
      font-weight: 500;
      cursor: pointer;
      transition: background var(--duration-fast) ease;
    }

    .btn:hover {
      background: var(--bg-hover);
    }

    .btn-primary {
      background: var(--accent);
      color: var(--accent-foreground);
    }

    .btn-primary:hover {
      background: var(--accent-hover);
    }

    .btn-danger {
      color: var(--danger);
    }

    .btn-danger:hover {
      background: var(--danger-subtle);
    }

    /* Message */
    .message {
      padding: 12px 16px;
      border-radius: var(--radius-md);
      font-size: 13px;
      margin-bottom: 20px;
    }

    .message.success {
      background: var(--ok-subtle);
      color: var(--ok);
      box-shadow: var(--glass-shadow-sm);
    }

    .message.error {
      background: var(--danger-subtle);
      color: var(--danger);
      box-shadow: var(--glass-shadow-sm);
    }

    /* Avatar section */
    .avatar-section {
      display: flex;
      align-items: center;
      gap: 20px;
      margin-bottom: 24px;
    }

    .avatar {
      width: 80px;
      height: 80px;
      border-radius: 50%;
      background: var(--accent-subtle);
      display: flex;
      align-items: center;
      justify-content: center;
      color: var(--accent);
      font-size: 32px;
      font-weight: 600;
    }

    .avatar-info h3 {
      font-size: 16px;
      font-weight: 600;
      color: var(--text-strong);
      margin: 0 0 4px;
    }

    .avatar-info p {
      font-size: 13px;
      color: var(--muted);
      margin: 0;
    }

    /* Danger zone */
    .danger-zone .card-header {
      background: var(--danger-subtle);
      box-shadow: 0 1px 0 var(--card-highlight);
    }

    .danger-zone .card-title {
      color: var(--danger);
    }

    .danger-zone .btn-row {
      background: var(--danger-subtle);
    }
  `]

  @state() activeTab = 'profile'
  @state() profile: UserProfile | null = null
  @state() loading = true
  @state() saving = false
  @state() message: { type: 'success' | 'error'; text: string } | null = null

  // Profile form
  @state() name = ''
  @state() email = ''
  @state() phone = ''

  // Password form
  @state() oldPassword = ''
  @state() newPassword = ''
  @state() confirmPassword = ''

  async connectedCallback() {
    super.connectedCallback()
    await this.loadProfile()
  }

  async loadProfile() {
    this.loading = true
    try {
      const response = await authApi.getProfile()
      if (response.result) {
        this.profile = response.result
        this.name = response.result.name || ''
        this.email = response.result.email || ''
        this.phone = response.result.phone || ''
      }
    } catch (err: any) {
      this.showMessage('error', err.message || '加载失败')
    } finally {
      this.loading = false
    }
  }

  async saveProfile() {
    this.saving = true
    this.message = null

    try {
      await authApi.updateProfile({
        name: this.name,
        email: this.email,
        phone: this.phone,
      })
      this.showMessage('success', '保存成功')
    } catch (err: any) {
      this.showMessage('error', err.message || '保存失败')
    } finally {
      this.saving = false
    }
  }

  async changePassword() {
    if (this.newPassword !== this.confirmPassword) {
      this.showMessage('error', '两次输入的密码不一致')
      return
    }

    if (this.newPassword.length < 6) {
      this.showMessage('error', '密码长度不能少于6位')
      return
    }

    this.saving = true
    this.message = null

    try {
      await authApi.changePassword({
        oldPassword: this.oldPassword,
        newPassword: this.newPassword,
      })
      this.showMessage('success', '密码修改成功')
      this.oldPassword = ''
      this.newPassword = ''
      this.confirmPassword = ''
    } catch (err: any) {
      this.showMessage('error', err.message || '修改失败')
    } finally {
      this.saving = false
    }
  }

  showMessage(type: 'success' | 'error', text: string) {
    this.message = { type, text }
    setTimeout(() => {
      this.message = null
    }, 3000)
  }

  render() {
    return html`
      <div class="page-header">
        <h1 class="page-title">设置</h1>
        <p class="page-subtitle">管理您的账户和偏好设置</p>
      </div>

      <div class="tabs">
        <button
          class="tab ${this.activeTab === 'profile' ? 'active' : ''}"
          @click=${() => this.activeTab = 'profile'}
        >
          个人资料
        </button>
        <button
          class="tab ${this.activeTab === 'security' ? 'active' : ''}"
          @click=${() => this.activeTab = 'security'}
        >
          安全设置
        </button>
      </div>

      ${this.activeTab === 'profile' ? this.renderProfileTab() : this.renderSecurityTab()}
    `
  }

  renderProfileTab() {
    return html`
      <div class="card">
        <div class="card-header">
          <h3 class="card-title">个人信息</h3>
        </div>
        <div class="card-body">
          ${this.message ? html`<div class="message ${this.message.type}">${this.message.text}</div>` : ''}

          <div class="avatar-section">
            <div class="avatar">${this.name ? this.name[0].toUpperCase() : 'U'}</div>
            <div class="avatar-info">
              <h3>${this.name || '用户'}</h3>
              <p>${this.email || '未设置邮箱'}</p>
            </div>
          </div>

          <div class="form-grid">
            <div class="form-group">
              <label class="form-label">用户名</label>
              <input
                type="text"
                class="form-input"
                .value=${this.name}
                @input=${(e: InputEvent) => this.name = (e.target as HTMLInputElement).value}
              />
            </div>
            <div class="form-group">
              <label class="form-label">邮箱</label>
              <input
                type="email"
                class="form-input"
                .value=${this.email}
                @input=${(e: InputEvent) => this.email = (e.target as HTMLInputElement).value}
              />
            </div>
            <div class="form-group">
              <label class="form-label">手机号</label>
              <input
                type="tel"
                class="form-input"
                .value=${this.phone}
                @input=${(e: InputEvent) => this.phone = (e.target as HTMLInputElement).value}
              />
            </div>
          </div>
        </div>
        <div class="btn-row">
          <button class="btn btn-primary" @click=${() => this.saveProfile()} ?disabled=${this.saving}>
            ${this.saving ? '保存中...' : '保存更改'}
          </button>
        </div>
      </div>
    `
  }

  renderSecurityTab() {
    return html`
      <div class="card">
        <div class="card-header">
          <h3 class="card-title">修改密码</h3>
        </div>
        <div class="card-body">
          ${this.message ? html`<div class="message ${this.message.type}">${this.message.text}</div>` : ''}

          <div class="form-grid">
            <div class="form-group full">
              <label class="form-label">当前密码</label>
              <input
                type="password"
                class="form-input"
                placeholder="请输入当前密码"
                .value=${this.oldPassword}
                @input=${(e: InputEvent) => this.oldPassword = (e.target as HTMLInputElement).value}
              />
            </div>
            <div class="form-group">
              <label class="form-label">新密码</label>
              <input
                type="password"
                class="form-input"
                placeholder="请输入新密码"
                .value=${this.newPassword}
                @input=${(e: InputEvent) => this.newPassword = (e.target as HTMLInputElement).value}
              />
              <span class="form-hint">至少6个字符</span>
            </div>
            <div class="form-group">
              <label class="form-label">确认新密码</label>
              <input
                type="password"
                class="form-input"
                placeholder="请再次输入新密码"
                .value=${this.confirmPassword}
                @input=${(e: InputEvent) => this.confirmPassword = (e.target as HTMLInputElement).value}
              />
            </div>
          </div>
        </div>
        <div class="btn-row">
          <button class="btn btn-primary" @click=${() => this.changePassword()} ?disabled=${this.saving}>
            ${this.saving ? '修改中...' : '修改密码'}
          </button>
        </div>
      </div>

      <div class="card danger-zone">
        <div class="card-header">
          <h3 class="card-title">危险区域</h3>
        </div>
        <div class="card-body">
          <p style="margin: 0 0 16px; color: var(--muted); font-size: 13px;">
            注销账户是一个不可逆的操作。注销后，所有您的数据将被永久删除。
          </p>
          <button class="btn btn-danger">注销账户</button>
        </div>
      </div>
    `
  }
}

declare global {
  interface HTMLElementTagNameMap {
    'settings-page': SettingsPage
  }
}
