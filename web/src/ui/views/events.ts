import { LitElement, html } from "lit";
import { customElement, state } from "lit/decorators.js";
import { eventApi } from "../../api/events.js";
import type { DeviceEvent } from "../../types/index.js";

@customElement("view-events")
export class EventsView extends LitElement {
  @state() loading = true;
  @state() error = "";
  @state() events: DeviceEvent[] = [];
  @state() page = 1;
  @state() pageSize = 20;
  @state() totalPages = 0;
  @state() totalCount = 0;
  @state() filterLevel = "";
  @state() filterType = "";

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
      const res = await eventApi.getEvents({
        page: this.page,
        pageSize: this.pageSize,
        level: this.filterLevel || undefined,
        eventType: this.filterType || undefined,
      });
      const data = res.result;
      if (data) {
        this.events = data.data || [];
        this.totalPages = data.pagination?.totalPages || 0;
        this.totalCount = data.pagination?.totalCount || 0;
      }
    } catch (err: any) {
      this.error = err.message || "加载事件日志失败";
    } finally {
      this.loading = false;
    }
  }

  levelColor(level?: string): string {
    switch (level?.toLowerCase?.()) {
      case "critical": return "var(--danger)";
      case "error": return "var(--danger)";
      case "warning": return "var(--warning)";
      default: return "var(--muted)";
    }
  }

  eventTypeLabel(type: string): string {
    switch (type) {
      case "alarm": return "告警";
      case "warning": return "警告";
      case "info": return "信息";
      case "error": return "错误";
      case "status_change": return "状态变更";
      case "command_executed": return "命令执行";
      default: return type;
    }
  }

  goToPage(p: number) {
    this.page = p;
    this.loadData();
  }

  render() {
    if (this.loading) {
      return html`
        <div class="page-loading">
          <span class="loading-spinner"></span>
          <span>加载中...</span>
        </div>
      `;
    }

    if (this.error) {
      return html`
        <div class="page-error">
          <div class="page-error__message">${this.error}</div>
          <button class="btn btn--primary" @click=${this.loadData}>重试</button>
        </div>
      `;
    }

    return html`
      <div class="filter-bar">
        <select class="select filter-bar__select" .value=${this.filterLevel} @change=${(e: Event) => { this.filterLevel = (e.target as HTMLSelectElement).value; this.page = 1; this.loadData(); }}>
          <option value="">全部级别</option>
          <option value="critical">严重</option>
          <option value="error">错误</option>
          <option value="warning">警告</option>
          <option value="info">信息</option>
        </select>
        <select class="select filter-bar__select" .value=${this.filterType} @change=${(e: Event) => { this.filterType = (e.target as HTMLSelectElement).value; this.page = 1; this.loadData(); }}>
          <option value="">全部类型</option>
          <option value="alarm">告警</option>
          <option value="warning">警告</option>
          <option value="info">信息</option>
          <option value="error">错误</option>
          <option value="status_change">状态变更</option>
          <option value="command_executed">命令执行</option>
        </select>
      </div>
      <div class="card table-container">
        <table class="data-table">
          <thead>
            <tr>
              <th>级别</th>
              <th>类型</th>
              <th>标题</th>
              <th>消息</th>
              <th>时间</th>
            </tr>
          </thead>
          <tbody>
            ${this.events.length === 0
              ? html`<tr><td colspan="5" class="empty-hint">暂无事件</td></tr>`
              : this.events.map(ev => html`
                <tr>
                  <td>
                    <span class="status-badge">
                      <span class="status-dot" style="background: ${this.levelColor(ev.level)};"></span>
                      <span class="status-badge__label">${ev.level}</span>
                    </span>
                  </td>
                  <td class="data-table__cell-sm">${this.eventTypeLabel(ev.eventType)}</td>
                  <td class="data-table__cell-sm">${ev.title}</td>
                  <td class="cell-truncate data-table__cell-sm">${ev.message}</td>
                  <td class="cell-muted">${ev.createdAt?.slice(0, 16)}</td>
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
    `;
  }
}
