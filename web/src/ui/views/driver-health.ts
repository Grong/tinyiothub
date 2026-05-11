import { LitElement, html } from "lit";
import { customElement, state } from "lit/decorators.js";
import { driverHealthApi, type DriverHealthInfo } from "../../api/driver-health.js";
import { error as toastError } from "../components/toast.js";

@customElement("view-driver-health")
export class DriverHealthView extends LitElement {
  @state() loading = true;
  @state() health: DriverHealthInfo[] = [];
  @state() workspaceId = "";
  @state() error = "";

  createRenderRoot() {
    return this;
  }

  connectedCallback() {
    super.connectedCallback();
    this.loadHealth();
  }

  async loadHealth() {
    this.loading = true;
    this.error = "";
    try {
      const res = await driverHealthApi.getWorkspaceHealth();
      this.workspaceId = res.result?.workspaceId ?? "";
      this.health = res.result?.drivers ?? [];
    } catch (e: any) {
      this.error = e.message || "加载健康状态失败";
      toastError(this.error);
    } finally {
      this.loading = false;
    }
  }

  statusColor(status: string): string {
    switch (status) {
      case "active": return "var(--success)";
      case "idle": return "var(--info)";
      case "error": return "var(--danger)";
      case "unloading": return "var(--warning)";
      default: return "var(--muted)";
    }
  }

  statusLabel(status: string): string {
    switch (status) {
      case "active": return "使用中";
      case "idle": return "空闲";
      case "error": return "故障";
      case "unloading": return "卸载中";
      default: return status;
    }
  }

  render() {
    return html`
      <div style="display: flex; justify-content: flex-end; margin-bottom: 16px;">
        <button class="btn btn--sm" @click=${() => this.loadHealth()}>刷新</button>
      </div>

      ${this.loading
        ? html`<div class="card">加载中...</div>`
        : this.error
          ? html`<div class="card" style="color: var(--danger);">${this.error}</div>`
          : this.renderTable()}
    `;
  }

  renderTable() {
    if (this.health.length === 0) {
      return html`<div class="card empty-hint">当前工作空间没有加载的动态驱动</div>`;
    }
    return html`
      <div class="card">
        <table class="data-table">
          <thead>
            <tr>
              <th>驱动名称</th>
              <th>版本</th>
              <th>加载时间</th>
              <th>引用计数</th>
              <th>状态</th>
            </tr>
          </thead>
          <tbody>
            ${this.health.map((h) => html`
              <tr>
                <td>${h.driverName}</td>
                <td>${h.version}</td>
                <td>${h.loadedAt}</td>
                <td>${h.refCount}</td>
                <td>
                  <span class="status-badge">
                    <span class="status-dot" style="background: ${this.statusColor(h.status)};"></span>
                    <span class="status-badge__label">${this.statusLabel(h.status)}</span>
                  </span>
                </td>
              </tr>
            `)}
          </tbody>
        </table>
      </div>
    `;
  }

  static styles = [];
}
