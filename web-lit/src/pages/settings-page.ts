import { LitElement, html} from 'lit'
import { customElement, state } from 'lit/decorators.js'
import { authApi, type UserProfile } from '../services/auth'
import { $user, type User } from '../stores/auth-store'

@customElement('settings-page')
export class SettingsPage extends LitElement {
  createRenderRoot() { return this }
  

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
