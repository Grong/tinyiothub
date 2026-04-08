import { LitElement, html, nothing } from "lit";
import { customElement, state } from "lit/decorators.js";
import { authApi } from "../../api/auth.js";
import { userApi } from "../../api/users.js";
import { apiKeyApi, type ApiKey } from "../../api/api-key.js";
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

  // API Keys
  @state() _apiKeys: ApiKey[] = [];
  @state() _keysLoading = false;
  @state() _showCreateDialog = false;
  @state() _createName = "";
  @state() _creating = false;
  @state() _showKeyModal = false;
  @state() _newKey: ApiKey | null = null;
  @state() _rawKey = "";
  @state() _showDeleteDialog = false;
  @state() _deleteTarget: ApiKey | null = null;
  @state() _revoking = false;
  @state() _copyingId: string | null = null;
  @state() _keyModalChecked = false;
  @state() _keyAutoCopied = false;

  createRenderRoot() {
    return this;
  }

  connectedCallback() {
    super.connectedCallback();
    this.loadProfile();
  }

  private getWorkspaceId(): string {
    return (
      localStorage.getItem("workspace-id") ||
      sessionStorage.getItem("workspace-id") ||
      "default"
    );
  }

  /** 将 key prefix 脱敏：前10字符明文 + 隐藏部分补足48位 * */
  private maskKeyPrefix(prefix: string): string {
    if (prefix.length <= 10) return "*".repeat(prefix.length);
    return prefix.slice(0, 10) + "*".repeat(48);
  }

  // === API Keys ===

  async loadApiKeys() {
    this._keysLoading = true;
    try {
      this._apiKeys = await apiKeyApi.list(this.getWorkspaceId());
    } catch (err: any) {
      toastError(err.message || "加载 API Keys 失败");
    } finally {
      this._keysLoading = false;
    }
  }

  async createApiKey() {
    if (!this._createName.trim()) {
      toastError("请输入 Key 名称");
      return;
    }
    this._creating = true;
    try {
      const result = await apiKeyApi.create(this.getWorkspaceId(), this._createName.trim());
      this._newKey = result.apiKey;
      this._rawKey = result.rawKey;
      this._showKeyModal = true;
      this._showCreateDialog = false;
      this._createName = "";
      this._apiKeys = [result.apiKey, ...this._apiKeys];
    } catch (err: any) {
      toastError(err.message || "创建失败");
    } finally {
      this._creating = false;
    }
  }

  async revokeApiKey() {
    if (!this._deleteTarget) return;
    this._revoking = true;
    try {
      await apiKeyApi.revoke(this._deleteTarget.id);
      this._apiKeys = this._apiKeys.filter(k => k.id !== this._deleteTarget!.id);
      this._showDeleteDialog = false;
      this._deleteTarget = null;
      success("API Key 已删除");
    } catch (err: any) {
      toastError(err.message || "删除失败");
    } finally {
      this._revoking = false;
    }
  }

  async copyKey(key: ApiKey) {
    this._copyingId = key.id;
    try {
      await navigator.clipboard.writeText(key.prefix);
      success("已复制到剪贴板");
    } catch {
      toastError("复制失败，请重试");
    } finally {
      setTimeout(() => (this._copyingId = null), 1500);
    }
  }

  async copyRawKey() {
    try {
      await navigator.clipboard.writeText(this._rawKey);
      success("已复制到剪贴板");
    } catch {
      toastError("复制失败，请手动复制");
    }
  }

  // === Profile ===

  async loadProfile() {
    this.loading = true;
    try {
      const res = await authApi.getCurrentUser();
      this.profile = res.result;
      if (this.profile) {
        this.name = this.profile.displayName || "";
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
      { key: "apiKeys", label: "API Keys" },
    ];
    return html`
      <div class="settings-tabs">
        ${tabs.map(t => html`
          <button
            class="settings-tab ${this.activeTab === t.key ? "active" : ""}"
            @click=${() => {
              this.activeTab = t.key as any;
              if (t.key === "apiKeys" && this._apiKeys.length === 0 && !this._keysLoading) {
                this.loadApiKeys();
              }
            }}
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
              : html`<span>${(p?.displayName || "?")[0].toUpperCase()}</span>`
            }
          </div>
          <div>
            <div style="font-weight: 600; font-size: 16px;">${p?.displayName || "-"}</div>
            <div style="font-size: 13px; color: var(--muted);">${!p?.isDisabled ? "管理员" : "用户"}</div>
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

  // === API Keys ===

  renderApiKeysTab() {
    return html`
      <div class="settings-section">
        <div class="settings-section-header">
          <div>
            <h3 class="settings-section-title">API Keys</h3>
            <p class="settings-section-desc">用于 AI Agent 认证的私有密钥。</p>
          </div>
          <button
            class="submit-btn"
            style="flex-shrink: 0; width: auto;"
            @click=${() => (this._showCreateDialog = true)}
          >+ 添加 Key</button>
        </div>

        ${this._keysLoading ? this.renderApiKeySkeleton() : nothing}
        ${!this._keysLoading && this._apiKeys.length === 0 ? this.renderApiKeyEmpty() : nothing}
        ${!this._keysLoading && this._apiKeys.length > 0 ? this.renderApiKeyList() : nothing}
      </div>

      ${this._showCreateDialog ? this.renderCreateKeyDialog() : nothing}
      ${this._showKeyModal && this._newKey ? this.renderKeyDisplayModal() : nothing}
      ${this._showDeleteDialog && this._deleteTarget ? this.renderDeleteConfirmDialog() : nothing}
    `;
  }

  renderApiKeySkeleton() {
    return html`
      <div class="api-key-list">
        ${[0, 1, 2].map(() => html`
          <div class="api-key-card skeleton">
            <div class="skeleton-line" style="width: 60%; height: 16px; margin-bottom: 8px;"></div>
            <div class="skeleton-line" style="width: 40%; height: 12px; margin-bottom: 12px;"></div>
            <div class="skeleton-line" style="width: 30%; height: 12px;"></div>
          </div>
        `)}
      </div>
    `;
  }

  renderApiKeyEmpty() {
    return html`
      <div class="api-key-empty">
        <div class="api-key-empty-icon">🔑</div>
        <div class="api-key-empty-title">还没有 API Key</div>
        <div class="api-key-empty-desc">创建第一个 Key，用于 AI Agent 认证。</div>
      </div>
    `;
  }

  renderApiKeyList() {
    return html`
      <div class="api-key-list" role="list">
        ${this._apiKeys.map(key => html`
          <div class="api-key-card" role="listitem">
            <div class="api-key-card-main">
              <div class="api-key-name">${key.name}</div>
              <div class="api-key-prefix">${this.maskKeyPrefix(key.prefix)}</div>
            </div>
            <div class="api-key-card-meta">
              <div class="api-key-status ${key.isRevoked ? "revoked" : "active"}">
                ${key.isRevoked ? "● 已删除" : "● 有效"}
              </div>
              <div class="api-key-date">创建于 ${new Date(key.createdAt).toLocaleDateString("zh-CN")}</div>
            </div>
            <div class="api-key-actions">
              <button
                class="api-key-btn"
                @click=${() => this.copyKey(key)}
                title="复制 Key"
              >
                ${this._copyingId === key.id
                  ? html`<span class="api-key-btn-copy-done">✓</span>`
                  : html`<span>📋</span>`}
              </button>
              ${!key.isRevoked ? html`
                <button
                  class="api-key-btn api-key-btn-delete"
                  @click=${() => {
                    this._deleteTarget = key;
                    this._showDeleteDialog = true;
                  }}
                  title="删除 Key"
                >🗑</button>
              ` : nothing}
            </div>
          </div>
        `)}
      </div>
    `;
  }

  renderCreateKeyDialog() {
    return html`
      <div class="modal-overlay" @click=${(e: MouseEvent) => {
        if (e.target === e.currentTarget) this._showCreateDialog = false;
      }}>
        <div class="modal-box" role="dialog" aria-labelledby="create-key-title">
          <div class="modal-header">
            <h3 id="create-key-title">创建 API Key</h3>
            <button class="modal-close" @click=${() => (this._showCreateDialog = false)}>×</button>
          </div>
          <div class="modal-body">
            <div class="form-group">
              <label class="form-label">Key 名称</label>
              <input
                class="form-input"
                type="text"
                placeholder="例如：Building A Agent"
                .value=${this._createName}
                @input=${(e: any) => (this._createName = e.target.value)}
                @keydown=${(e: KeyboardEvent) => {
                  if (e.key === "Enter") this.createApiKey();
                }}
              />
            </div>
          </div>
          <div class="modal-footer">
            <button class="btn-secondary" @click=${() => (this._showCreateDialog = false)}>取消</button>
            <button
              class="submit-btn"
              ?disabled=${this._creating}
              @click=${() => this.createApiKey()}
            >${this._creating ? "创建中..." : "创建 Key"}</button>
          </div>
        </div>
      </div>
    `;
  }

  renderKeyDisplayModal() {
    // Auto-copy on first render
    if (!this._keyAutoCopied && this._rawKey) {
      this._keyAutoCopied = true;
      this.copyRawKey();
    }
    return html`
      <div class="modal-overlay">
        <div class="modal-box" role="dialog" aria-labelledby="key-display-title">
          <div class="modal-header">
            <h3 id="key-display-title">✓ API Key 已创建</h3>
            <button
              class="modal-close"
              @click=${() => {
                if (this._keyModalChecked) {
                  this._showKeyModal = false;
                  this._newKey = null;
                  this._rawKey = "";
                  this._keyModalChecked = false;
                  this._keyAutoCopied = false;
                }
              }}
            >×</button>
          </div>
          <div class="modal-body">
            <p class="modal-desc">此 Key 将用于 AI Agent 认证。已在打开时自动复制到剪贴板。</p>
            <div class="api-key-raw-box">${this._rawKey}</div>
            <p class="modal-warning">关闭后将无法再次查看完整 Key，请务必复制保存。</p>
            <label class="checkbox-label">
              <input
                type="checkbox"
                .checked=${this._keyModalChecked}
                @change=${(e: Event) => {
                  this._keyModalChecked = (e.target as HTMLInputElement).checked;
                }}
              />
              <span>我已复制，关闭此窗口</span>
            </label>
          </div>
          <div class="modal-footer">
            <button
              class="submit-btn"
              ?disabled=${!this._keyModalChecked}
              @click=${() => {
                this._showKeyModal = false;
                this._newKey = null;
                this._rawKey = "";
                this._keyModalChecked = false;
                this._keyAutoCopied = false;
              }}
            >关闭</button>
          </div>
        </div>
      </div>
    `;
  }

  renderDeleteConfirmDialog() {
    return html`
      <div class="modal-overlay" @click=${(e: MouseEvent) => {
        if (e.target === e.currentTarget) this._showDeleteDialog = false;
      }}>
        <div class="modal-box" role="dialog" aria-labelledby="delete-key-title">
          <div class="modal-header">
            <h3 id="delete-key-title">确认删除 API Key</h3>
            <button class="modal-close" @click=${() => (this._showDeleteDialog = false)}>×</button>
          </div>
          <div class="modal-body">
            <p>确定要删除 <strong>"${this._deleteTarget?.name}"</strong> 吗？</p>
            <p class="modal-warning">此操作无法撤销，使用此 Key 的 AI Agent 将立即失去认证能力。</p>
          </div>
          <div class="modal-footer">
            <button class="btn-secondary" @click=${() => (this._showDeleteDialog = false)}>取消</button>
            <button
              class="submit-btn"
              style="background: #dc2626;"
              ?disabled=${this._revoking}
              @click=${() => this.revokeApiKey()}
            >${this._revoking ? "删除中..." : "确认删除"}</button>
          </div>
        </div>
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
          ${this.activeTab === "apiKeys" ? this.renderApiKeysTab() : nothing}
        </div>
      </div>
    `;
  }
}
