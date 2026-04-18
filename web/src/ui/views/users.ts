import { LitElement, html, nothing } from "lit";
import { customElement, state } from "lit/decorators.js";
import { userApi } from "../../api/users.js";
import type { User, CreateUserRequest } from "../../types/index.js";
import { success, error as toastError } from "../components/toast.js";

@customElement("view-users")
export class UsersView extends LitElement {
  @state() loading = true;
  @state() error = "";
  @state() users: User[] = [];
  @state() searchKeyword = "";
  @state() page = 1;
  @state() pageSize = 20;
  @state() totalPages = 0;
  @state() totalCount = 0;

  @state() showModal = false;
  @state() editingUser: User | null = null;
  @state() saving = false;
  @state() formName = "";
  @state() formUsername = "";
  @state() formPassword = "";
  @state() formEmail = "";
  @state() formPhone = "";
  @state() formRole = "";

  @state() showPasswordModal = false;
  @state() passwordUser: User | null = null;
  @state() newPwOld = "";
  @state() newPwNew = "";

  createRenderRoot() {
    return this;
  }

  connectedCallback() {
    super.connectedCallback();
    this.loadData();
  }

  async loadData() {
    this.loading = true;
    this.error = "";
    try {
      const res = await userApi.getUsers({ page: this.page, pageSize: this.pageSize });
      const data = res.result;
      if (data) {
        this.users = data.data || [];
        this.totalPages = data.pagination?.totalPages || 1;
        this.totalCount = data.pagination?.totalCount || this.users.length;
      }
    } catch (err: any) {
      this.error = err.message || "加载用户列表失败";
    } finally {
      this.loading = false;
    }
  }

  goToPage(p: number) {
    this.page = p;
    this.loadData();
  }

  get filteredUsers(): User[] {
    if (!this.searchKeyword) return this.users;
    const kw = this.searchKeyword.toLowerCase();
    return this.users.filter(u =>
      (u.displayName || u.id).toLowerCase().includes(kw) ||
      (u.email || "").toLowerCase().includes(kw) ||
      (u.phone || "").toLowerCase().includes(kw)
    );
  }

  openCreate() {
    this.editingUser = null;
    this.formName = "";
    this.formUsername = "";
    this.formPassword = "";
    this.formEmail = "";
    this.formPhone = "";
    this.formRole = "";
    this.showModal = true;
  }

  openEdit(u: User) {
    this.editingUser = u;
    this.formName = u.displayName || "";
    this.formUsername = "";
    this.formPassword = "";
    this.formEmail = u.email || "";
    this.formPhone = u.phone || "";
    this.formRole = "";
    this.showModal = true;
  }

  closeModal() {
    this.showModal = false;
    this.editingUser = null;
  }

  async saveForm() {
    if (!this.formName.trim()) return;
    this.saving = true;
    try {
      if (this.editingUser) {
        await userApi.updateUser(this.editingUser.id, {
          name: this.formName,
          email: this.formEmail || undefined,
          phone: this.formPhone || undefined,
        });
        success("用户已更新");
      } else {
        if (!this.formUsername.trim() || !this.formPassword.trim()) {
          toastError("用户名和密码不能为空");
          this.saving = false;
          return;
        }
        const payload: CreateUserRequest = {
          name: this.formName,
          username: this.formUsername,
          password: this.formPassword,
          email: this.formEmail || undefined,
          phone: this.formPhone || undefined,
          role: this.formRole || undefined,
        };
        await userApi.createUser(payload);
        success("用户已创建");
      }
      this.closeModal();
      await this.loadData();
    } catch (err: any) {
      toastError(err.message || "操作失败");
    } finally {
      this.saving = false;
    }
  }

  async toggleDisable(u: User) {
    const action = u.isDisabled ? "启用" : "禁用";
    if (!confirm(`确定要${action}用户 "${u.displayName || u.id}" 吗？`)) return;
    try {
      await userApi.updateUser(u.id, { isDisabled: !u.isDisabled });
      success(`用户已${action}`);
      await this.loadData();
    } catch (err: any) {
      toastError(err.message || `${action}失败`);
    }
  }

  async deleteUser(u: User) {
    if (!confirm(`确定要删除用户 "${u.displayName || u.id}" 吗？此操作不可撤销。`)) return;
    try {
      await userApi.deleteUser(u.id);
      success("用户已删除");
      await this.loadData();
    } catch (err: any) {
      toastError(err.message || "删除失败");
    }
  }

  openChangePassword(u: User) {
    this.passwordUser = u;
    this.newPwOld = "";
    this.newPwNew = "";
    this.showPasswordModal = true;
  }

  closePasswordModal() {
    this.showPasswordModal = false;
    this.passwordUser = null;
  }

  async savePassword() {
    if (!this.newPwNew.trim()) return;
    this.saving = true;
    try {
      await userApi.changePassword(this.passwordUser!.id, {
        oldPassword: this.newPwOld,
        newPassword: this.newPwNew,
      });
      success("密码已修改");
      this.closePasswordModal();
    } catch (err: any) {
      toastError(err.message || "修改密码失败");
    } finally {
      this.saving = false;
    }
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

    if (this.error) {
      return html`
        <div style="text-align: center; padding: 60px;">
          <div style="color: var(--danger); margin-bottom: 12px;">${this.error}</div>
          <button class="btn btn--primary" @click=${this.loadData}>重试</button>
        </div>
      `;
    }

    return html`
      <div style="display: flex; gap: 12px; margin-bottom: 16px; align-items: center;">
        <input
          type="text"
          placeholder="搜索用户名、邮箱、手机..."
          .value=${this.searchKeyword}
          @input=${(e: Event) => { this.searchKeyword = (e.target as HTMLInputElement).value; this.page = 1; this.loadData(); }}
          style="flex: 1; max-width: 300px;"
        />
        <button class="btn btn--primary" @click=${this.openCreate}>新建用户</button>
      </div>
      <div class="card" style="overflow: hidden;">
        <table style="width: 100%; border-collapse: collapse;">
          <thead>
            <tr style="border-bottom: 1px solid var(--border);">
              <th style="padding: 12px 16px; text-align: left; font-size: 13px; color: var(--muted); font-weight: 500;">用户</th>
              <th style="padding: 12px 16px; text-align: left; font-size: 13px; color: var(--muted); font-weight: 500;">邮箱</th>
              <th style="padding: 12px 16px; text-align: left; font-size: 13px; color: var(--muted); font-weight: 500;">手机</th>
              <th style="padding: 12px 16px; text-align: left; font-size: 13px; color: var(--muted); font-weight: 500;">状态</th>
              <th style="padding: 12px 16px; text-align: left; font-size: 13px; color: var(--muted); font-weight: 500;">最后登录</th>
              <th style="padding: 12px 16px; text-align: right; font-size: 13px; color: var(--muted); font-weight: 500;">操作</th>
            </tr>
          </thead>
          <tbody>
            ${this.filteredUsers.length === 0
              ? html`<tr><td colspan="6" style="padding: 40px; text-align: center; color: var(--muted);">暂无用户</td></tr>`
              : this.filteredUsers.map(u => html`
                <tr style="border-bottom: 1px solid var(--border);">
                  <td style="padding: 12px 16px;">
                    <div style="display: flex; align-items: center; gap: 10px;">
                      <span style="width: 32px; height: 32px; border-radius: 50%; background: var(--accent-subtle, var(--primary)); color: var(--accent, #fff); display: flex; align-items: center; justify-content: center; font-size: 14px; font-weight: 600;">
                        ${(u.displayName || u.id).charAt(0).toUpperCase()}
                      </span>
                      <div>
                        <div style="font-weight: 500;">${u.displayName || u.id}</div>
                        <div style="font-size: 12px; color: var(--muted);">${u.id}</div>
                      </div>
                    </div>
                  </td>
                  <td style="padding: 12px 16px; font-size: 13px;">${u.email || "-"}</td>
                  <td style="padding: 12px 16px; font-size: 13px;">${u.phone || "-"}</td>
                  <td style="padding: 12px 16px;">
                    <span style="display: inline-flex; align-items: center; gap: 6px; font-size: 13px;">
                      <span style="width: 8px; height: 8px; border-radius: 50%; background: ${u.isDisabled ? 'var(--muted)' : 'var(--success)'};"></span>
                      ${u.isDisabled ? "已禁用" : "正常"}
                    </span>
                  </td>
                  <td style="padding: 12px 16px; font-size: 13px; color: var(--muted);">${u.dateLastLogon?.slice(0, 16) || "-"}</td>
                  <td style="padding: 12px 16px; text-align: right;">
                    <button class="btn btn--ghost btn--sm" style="font-size: 12px;" @click=${() => this.openEdit(u)}>编辑</button>
                    <button class="btn btn--ghost btn--sm" style="font-size: 12px;" @click=${() => this.openChangePassword(u)}>改密</button>
                    <button class="btn btn--ghost btn--sm" style="font-size: 12px;" @click=${() => this.toggleDisable(u)}>
                      ${u.isDisabled ? "启用" : "禁用"}
                    </button>
                    <button class="btn btn--ghost btn--sm" style="font-size: 12px; color: var(--danger);" @click=${() => this.deleteUser(u)}>删除</button>
                  </td>
                </tr>
              `)}
          </tbody>
        </table>
      </div>
      ${this.totalPages > 1 ? html`
        <div class="pagination">
          <button class="btn btn--ghost btn--sm" ?disabled=${this.page <= 1} @click=${() => this.goToPage(this.page - 1)}>上一页</button>
          <span class="pagination-info">第 ${this.page} / ${this.totalPages} 页，共 ${this.totalCount} 条</span>
          <button class="btn btn--ghost btn--sm" ?disabled=${this.page >= this.totalPages} @click=${() => this.goToPage(this.page + 1)}>下一页</button>
        </div>
      ` : ""}
      ${this.showModal ? this.renderModal() : nothing}
      ${this.showPasswordModal ? this.renderPasswordModal() : nothing}
    `;
  }

  renderModal() {
    return html`
      <div class="modal-overlay" role="dialog" aria-modal="true" aria-label=${this.editingUser ? "编辑用户" : "新建用户"} @click=${this.closeModal}>
        <div class="modal" @click=${(e: Event) => e.stopPropagation()}>
          <div class="modal-header">${this.editingUser ? "编辑用户" : "新建用户"}</div>
          <div class="modal-body">
            <div class="field">
              <span>名称</span>
              <input type="text" placeholder="用户名称" .value=${this.formName} @input=${(e: any) => { this.formName = e.target.value; }} />
            </div>
            ${!this.editingUser ? html`
              <div class="field" style="margin-top: 12px;">
                <span>用户名（登录账号）</span>
                <input type="text" placeholder="登录用户名" .value=${this.formUsername} @input=${(e: any) => { this.formUsername = e.target.value; }} />
              </div>
              <div class="field" style="margin-top: 12px;">
                <span>密码</span>
                <input type="password" placeholder="登录密码" .value=${this.formPassword} @input=${(e: any) => { this.formPassword = e.target.value; }} />
              </div>
            ` : nothing}
            <div class="field" style="margin-top: 12px;">
              <span>邮箱</span>
              <input type="email" placeholder="可选" .value=${this.formEmail} @input=${(e: any) => { this.formEmail = e.target.value; }} />
            </div>
            <div class="field" style="margin-top: 12px;">
              <span>手机号</span>
              <input type="text" placeholder="可选" .value=${this.formPhone} @input=${(e: any) => { this.formPhone = e.target.value; }} />
            </div>
            ${!this.editingUser ? html`
              <div class="field" style="margin-top: 12px;">
                <span>角色</span>
                <input type="text" placeholder="如 admin, operator" .value=${this.formRole} @input=${(e: any) => { this.formRole = e.target.value; }} />
              </div>
            ` : nothing}
          </div>
          <div class="modal-footer">
            <button class="btn btn--ghost" @click=${this.closeModal}>取消</button>
            <button class="btn btn--primary" ?disabled=${this.saving || !this.formName.trim()} @click=${this.saveForm}>
              ${this.saving ? "保存中..." : "保存"}
            </button>
          </div>
        </div>
      </div>
    `;
  }

  renderPasswordModal() {
    return html`
      <div class="modal-overlay" role="dialog" aria-modal="true" aria-label="修改密码" @click=${this.closePasswordModal}>
        <div class="modal" @click=${(e: Event) => e.stopPropagation()}>
          <div class="modal-header">修改密码 — ${this.passwordUser?.displayName || this.passwordUser?.id}</div>
          <div class="modal-body">
            <div class="field">
              <span>旧密码</span>
              <input type="password" placeholder="输入旧密码" .value=${this.newPwOld} @input=${(e: any) => { this.newPwOld = e.target.value; }} />
            </div>
            <div class="field" style="margin-top: 12px;">
              <span>新密码</span>
              <input type="password" placeholder="输入新密码" .value=${this.newPwNew} @input=${(e: any) => { this.newPwNew = e.target.value; }} />
            </div>
          </div>
          <div class="modal-footer">
            <button class="btn btn--ghost" @click=${this.closePasswordModal}>取消</button>
            <button class="btn btn--primary" ?disabled=${this.saving || !this.newPwNew.trim()} @click=${this.savePassword}>
              ${this.saving ? "保存中..." : "修改密码"}
            </button>
          </div>
        </div>
      </div>
    `;
  }
}
