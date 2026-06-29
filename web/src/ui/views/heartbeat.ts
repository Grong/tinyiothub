import { LitElement, html, nothing } from "lit";
import { customElement, state } from "lit/decorators.js";
import { unsafeHTML } from "lit/directives/unsafe-html.js";
import { repeat } from "lit/directives/repeat.js";
import { marked } from "marked";
import DOMPurify from "dompurify";
import { apiGet, apiPut, apiPost } from "../../api/client.js";

marked.setOptions({ async: false, gfm: true });

function md(text: string): string {
  try {
    return DOMPurify.sanitize(marked.parse(text) as string);
  } catch {
    return DOMPurify.sanitize(text);
  }
}

// ── Types ──

interface HeartbeatConfig {
  enabled: boolean;
  intervalMinutes: number;
  workspaceId: string;
  agentId: string;
  tasks: HeartbeatTask[];
}

interface HeartbeatTask {
  priority: string;
  text: string;
  paused: boolean;
}

interface HeartbeatExecutionRecord {
  timestamp: string;
  taskCount: number;
  status: string;
  errorMessage?: string;
  result?: string;
}

interface PendingProposal {
  proposalId: string;
  status: string;
  level: string;
  toolName: string;
  deviceId: string;
  deviceName: string;
  summary: string;
  reason: string;
  risk: string;
  createdAt: string;
}

// ── Component ──

@customElement("view-heartbeat")
export class ViewHeartbeat extends LitElement {
  @state() private config: HeartbeatConfig | null = null;
  @state() private logs: HeartbeatExecutionRecord[] = [];
  @state() private approvals: PendingProposal[] = [];
  @state() private loading = true;
  @state() private error: string | null = null;
  @state() private workspaces: { id: string; name: string }[] = [];
  @state() private selectedWsId = "";
  @state() private wsLoading = true;
  @state() private rightTab: "approvals" | "history" = "approvals";

  createRenderRoot() {
    return this;
  }

  connectedCallback(): void {
    super.connectedCallback();
    this.loadWorkspaces();
  }

  // ── Data loading ──

  private async loadWorkspaces(): Promise<void> {
    this.wsLoading = true;
    try {
      const res = await apiGet<{ agents: { id: string; name?: string; workspaceId?: string }[] }>("/agents");
      const agents = res.result?.agents || [];
      const seen = new Set<string>();
      const wsList: { id: string; name: string }[] = [];
      for (const a of agents) {
        const wsId = a.workspaceId || a.id;
        if (seen.has(wsId)) continue;
        seen.add(wsId);
        wsList.push({ id: wsId, name: a.name || wsId });
      }
      this.workspaces = wsList;
      if (wsList.length > 0 && !this.selectedWsId) {
        this.selectedWsId = wsList[0].id;
        await this.loadAll();
      }
    } catch (err) {
      this.error = String(err);
    } finally {
      this.wsLoading = false;
    }
  }

  private async loadAll(): Promise<void> {
    if (!this.selectedWsId) return;
    this.loading = true;
    this.error = null;
    try {
      const [cfgRes, logsRes, apprRes] = await Promise.all([
        apiGet<HeartbeatConfig>(`/workspaces/${this.selectedWsId}/heartbeat/config`),
        apiGet<{ logs: HeartbeatExecutionRecord[] }>(`/workspaces/${this.selectedWsId}/heartbeat/logs`),
        apiGet<{ proposals: PendingProposal[] }>(`/workspaces/${this.selectedWsId}/heartbeat/approvals`),
      ]);
      if (cfgRes.result && typeof cfgRes.result.tasks === "string") {
        cfgRes.result.tasks = JSON.parse(cfgRes.result.tasks);
      }
      this.config = cfgRes.result || null;
      this.logs = logsRes.result?.logs ?? [];
      this.approvals = apprRes.result?.proposals ?? [];
    } catch (err) {
      this.error = String(err);
    } finally {
      this.loading = false;
    }
  }

  // ── Actions ──

  private onWorkspaceChange(wsId: string): void {
    this.selectedWsId = wsId;
    this.config = null;
    this.logs = [];
    this.approvals = [];
    this.loadAll();
  }

  private async onToggleHeartbeat(enabled: boolean): Promise<void> {
    if (!this.selectedWsId) return;
    await apiPut(`/workspaces/${this.selectedWsId}/heartbeat/config`, { enabled });
    await this.loadAll();
  }

  private async onChangeInterval(intervalMinutes: number): Promise<void> {
    if (!this.selectedWsId) return;
    await apiPut(`/workspaces/${this.selectedWsId}/heartbeat/config`, { intervalMinutes });
    await this.loadAll();
  }

  private async onToggleTask(index: number, paused: boolean): Promise<void> {
    if (!this.config || !this.selectedWsId) return;
    const tasks = [...this.config.tasks];
    tasks[index] = { ...tasks[index], paused };
    await apiPut(`/workspaces/${this.selectedWsId}/heartbeat/tasks`, { tasks });
    await this.loadAll();
  }

  private async onAddTask(): Promise<void> {
    if (!this.config || !this.selectedWsId) return;
    const tasks = [...this.config.tasks, { priority: "medium", text: "新任务", paused: false }];
    await apiPut(`/workspaces/${this.selectedWsId}/heartbeat/tasks`, { tasks });
    await this.loadAll();
  }

  private async onRemoveTask(index: number): Promise<void> {
    if (!this.config || !this.selectedWsId) return;
    const task = this.config.tasks[index];
    if (!confirm(`确定要删除任务"${task?.text || ""}"吗？`)) return;
    const tasks = this.config.tasks.filter((_, i) => i !== index);
    await apiPut(`/workspaces/${this.selectedWsId}/heartbeat/tasks`, { tasks });
    await this.loadAll();
  }

  private async onUpdateTask(index: number, patch: Partial<HeartbeatTask>): Promise<void> {
    if (!this.config || !this.selectedWsId) return;
    const tasks = [...this.config.tasks];
    tasks[index] = { ...tasks[index], ...patch };
    await apiPut(`/workspaces/${this.selectedWsId}/heartbeat/tasks`, { tasks });
    await this.loadAll();
  }

  private async onApprove(proposalId: string): Promise<void> {
    if (!this.selectedWsId) return;
    await apiPost(`/workspaces/${this.selectedWsId}/heartbeat/approvals/${proposalId}/approve`);
    await this.loadAll();
  }

  private async onReject(proposalId: string): Promise<void> {
    if (!this.selectedWsId) return;
    await apiPost(`/workspaces/${this.selectedWsId}/heartbeat/approvals/${proposalId}/reject`);
    await this.loadAll();
  }

  // ── Render ──

  render() {
    if (this.wsLoading) {
      return html`
        <div class="hb-page">
          <div class="hb-loading">
            <span class="hb-spinner"></span>
            加载工作空间…
          </div>
        </div>
      `;
    }

    if (this.workspaces.length === 0 && !this.wsLoading) {
      return html`
        <div class="hb-page">
          <div class="hb-empty">
            <svg class="hb-empty-icon" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.5">
              <path d="M12 9v4m0 4h.01M21 12a9 9 0 11-18 0 9 9 0 0118 0z"/>
            </svg>
            <span>暂无可用工作空间</span>
            <span class="hb-empty-sub">请先在 Agent 管理中创建 Agent 和工作空间</span>
          </div>
        </div>
      `;
    }

    const active = this.config?.enabled ?? false;

    return html`
      <div class="hb-page">
        <!-- Workspace selector bar -->
        <div class="hb-topbar">
          <div class="hb-ws-select">
            <svg class="hb-ws-icon" viewBox="0 0 20 20" fill="currentColor" width="16" height="16">
              <path fill-rule="evenodd" d="M3 4a1 1 0 011-1h12a1 1 0 011 1v2a1 1 0 01-1 1H4a1 1 0 01-1-1V4zm0 6a1 1 0 011-1h12a1 1 0 011 1v2a1 1 0 01-1 1H4a1 1 0 01-1-1v-2zm0 6a1 1 0 011-1h12a1 1 0 011 1v2a1 1 0 01-1 1H4a1 1 0 01-1-1v-2z" clip-rule="evenodd"/>
            </svg>
            <select
              class="hb-ws-dropdown"
              .value=${this.selectedWsId}
              @change=${(e: Event) => this.onWorkspaceChange((e.target as HTMLSelectElement).value)}
            >
              ${this.workspaces.map(
                (ws) => html`<option value=${ws.id}>${ws.name}</option>`
              )}
            </select>
          </div>
          <div class="hb-status ${active ? "is-active" : ""}">
            <span class="hb-pulse ${active ? "is-active" : ""}"></span>
            <span class="hb-status-text">${active ? "AI 巡检运行中" : "AI 巡检已停止"}</span>
            ${active
              ? html`<span class="hb-interval-badge">每 ${this.config?.intervalMinutes ?? 30} 分钟</span>`
              : nothing}
          </div>
        </div>

        ${this.loading && !this.config
          ? this.renderSkeleton()
          : this.error && !this.config
            ? this.renderError()
            : this.renderContent()}
      </div>
    `;
  }

  private renderSkeleton() {
    return html`
      <div class="hb-grid">
        <div class="hb-grid-left">
          <div class="hb-card">
            <div class="hb-skeleton hb-skeleton--toggle"></div>
            <div class="hb-skeleton hb-skeleton--select" style="margin-top:12px"></div>
          </div>
          <div class="hb-card">
            <div class="hb-skeleton hb-skeleton--row"></div>
            <div class="hb-skeleton hb-skeleton--row"></div>
            <div class="hb-skeleton hb-skeleton--row"></div>
          </div>
        </div>
        <div class="hb-grid-right">
          <div class="hb-card">
            <div class="hb-skeleton hb-skeleton--row"></div>
            <div class="hb-skeleton hb-skeleton--row"></div>
          </div>
          <div class="hb-card">
            <div class="hb-skeleton hb-skeleton--row"></div>
            <div class="hb-skeleton hb-skeleton--row"></div>
            <div class="hb-skeleton hb-skeleton--row"></div>
          </div>
        </div>
      </div>
    `;
  }

  private renderError() {
    return html`
      <div class="hb-card">
        <div class="hb-empty">
          <svg class="hb-empty-icon" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.5">
            <path d="M12 9v4m0 4h.01M21 12a9 9 0 11-18 0 9 9 0 0118 0z"/>
          </svg>
          <span>${this.error}</span>
        </div>
      </div>
    `;
  }

  private renderContent() {
    const config = this.config;
    const approvals = this.approvals;
    const logs = this.logs;
    const intervals = [5, 15, 30, 60];

    return html`
      <div class="hb-grid">
        <!-- Left column: Config + Tasks -->
        <div class="hb-grid-left">
          <!-- Config card -->
          <div class="hb-card ${config?.enabled ? "hb-card--active" : ""}">
            <div class="hb-card__header">
              <svg viewBox="0 0 20 20" fill="currentColor" width="14" height="14">
                <path fill-rule="evenodd" d="M11.49 3.17c-.38-1.56-2.6-1.56-2.98 0a1.532 1.532 0 01-2.286.948c-1.372-.836-2.942.734-2.106 2.106.54.886.061 2.042-.947 2.287-1.561.379-1.561 2.6 0 2.978a1.532 1.532 0 01.947 2.287c-.836 1.372.734 2.942 2.106 2.106a1.532 1.532 0 012.287.947c.379 1.561 2.6 1.561 2.978 0a1.533 1.533 0 012.287-.947c1.372.836 2.942-.734 2.106-2.106a1.533 1.533 0 01.947-2.287c1.561-.379 1.561-2.6 0-2.978a1.532 1.532 0 01-.947-2.287c.836-1.372-.734-2.942-2.106-2.106a1.532 1.532 0 01-2.287-.947zM10 13a3 3 0 100-6 3 3 0 000 6z" clip-rule="evenodd"/>
              </svg>
              <span>巡检配置</span>
            </div>

            <div class="hb-controls">
              <label class="hb-toggle">
                <input
                  type="checkbox"
                  class="hb-toggle__input"
                  .checked=${config?.enabled ?? false}
                  @change=${(e: Event) => this.onToggleHeartbeat((e.target as HTMLInputElement).checked)}
                />
                <span class="hb-toggle__track">
                  <span class="hb-toggle__thumb"></span>
                </span>
                <span class="hb-toggle__label">${config?.enabled ? "运行中" : "已停止"}</span>
              </label>

              <label class="hb-interval">
                <svg viewBox="0 0 20 20" fill="currentColor" width="14" height="14">
                  <path fill-rule="evenodd" d="M10 18a8 8 0 100-16 8 8 0 000 16zm1-12a1 1 0 10-2 0v4a1 1 0 00.293.707l2.828 2.829a1 1 0 101.415-1.415L11 9.586V6z" clip-rule="evenodd"/>
                </svg>
                <select
                  class="hb-interval__select"
                  .value=${String(config?.intervalMinutes ?? 30)}
                  @change=${(e: Event) => this.onChangeInterval(parseInt((e.target as HTMLSelectElement).value, 10))}
                >
                  ${intervals.map(
                    (m) => html`<option value=${m}>${m} 分钟</option>`
                  )}
                </select>
              </label>
            </div>
          </div>

          <!-- Tasks card -->
          <div class="hb-card">
            <div class="hb-card__header">
              <svg viewBox="0 0 20 20" fill="currentColor" width="14" height="14">
                <path fill-rule="evenodd" d="M16.707 5.293a1 1 0 010 1.414l-8 8a1 1 0 01-1.414 0l-4-4a1 1 0 011.414-1.414L8 12.586l7.293-7.293a1 1 0 011.414 0z" clip-rule="evenodd"/>
              </svg>
              <span>巡检任务</span>
              <span class="hb-card__count">${(config?.tasks ?? []).length}</span>
              <button class="hb-task-add" @click=${() => this.onAddTask()}>
                <svg viewBox="0 0 20 20" fill="currentColor" width="12" height="12">
                  <path fill-rule="evenodd" d="M10 3a1 1 0 011 1v5h5a1 1 0 110 2h-5v5a1 1 0 11-2 0v-5H4a1 1 0 110-2h5V4a1 1 0 011-1z" clip-rule="evenodd"/>
                </svg>
                添加
              </button>
            </div>

            <div class="hb-tasks">
              ${repeat(
                config?.tasks ?? [],
                (_t: HeartbeatTask, i: number) => `task-${i}`,
                (task: HeartbeatTask, i: number) => html`
                  <div class="hb-task ${task.paused ? "is-paused" : ""}">
                    <button
                      class="hb-task__pause ${task.paused ? "is-paused" : ""}"
                      title=${task.paused ? "恢复" : "暂停"}
                      @click=${() => this.onToggleTask(i, !task.paused)}
                    >
                      ${task.paused
                        ? html`<svg viewBox="0 0 20 20" fill="currentColor" width="12" height="12"><path d="M6.3 2.841A1.5 1.5 0 004 4.11v11.78a1.5 1.5 0 002.3 1.269l9.344-5.89a1.5 1.5 0 000-2.538L6.3 2.84z"/></svg>`
                        : html`<svg viewBox="0 0 20 20" fill="currentColor" width="12" height="12"><path d="M5.75 3a.75.75 0 00-.75.75v12.5c0 .414.336.75.75.75h1.5a.75.75 0 00.75-.75V3.75A.75.75 0 007.25 3h-1.5zM12.75 3a.75.75 0 00-.75.75v12.5c0 .414.336.75.75.75h1.5a.75.75 0 00.75-.75V3.75a.75.75 0 00-.75-.75h-1.5z"/></svg>`}
                    </button>
                    <select
                      class="hb-task__priority ${task.priority}"
                      .value=${task.priority}
                      @change=${(e: Event) => this.onUpdateTask(i, { priority: (e.target as HTMLSelectElement).value })}
                    >
                      <option value="high">高优先级</option>
                      <option value="medium">中优先级</option>
                      <option value="low">低优先级</option>
                    </select>
                    <input
                      type="text"
                      class="hb-task__text"
                      .value=${task.text}
                      placeholder="描述任务…"
                      @change=${(e: Event) => this.onUpdateTask(i, { text: (e.target as HTMLInputElement).value })}
                    />
                    <button class="hb-task__remove" title="删除" @click=${() => this.onRemoveTask(i)}>
                      <svg viewBox="0 0 20 20" fill="currentColor" width="12" height="12">
                        <path fill-rule="evenodd" d="M4.293 4.293a1 1 0 011.414 0L10 8.586l4.293-4.293a1 1 0 111.414 1.414L11.414 10l4.293 4.293a1 1 0 01-1.414 1.414L10 11.414l-4.293 4.293a1 1 0 01-1.414-1.414L8.586 10 4.293 5.707a1 1 0 010-1.414z" clip-rule="evenodd"/>
                      </svg>
                    </button>
                  </div>
                `
              )}
              ${(config?.tasks ?? []).length === 0
                ? html`<div class="hb-task--empty">暂无巡检任务，点击"添加"创建</div>`
                : nothing}
            </div>
          </div>
        </div>

        <!-- Right column: Tabs -->
        <div class="hb-grid-right">
          <div class="hb-card hb-card--fill">
            <div class="hb-tabs">
              <button
                class="hb-tab ${this.rightTab === "approvals" ? "is-active" : ""}"
                @click=${() => { this.rightTab = "approvals"; }}
              >
                待审批操作
                ${approvals.length > 0
                  ? html`<span class="hb-tab__badge">${approvals.length}</span>`
                  : nothing}
              </button>
              <button
                class="hb-tab ${this.rightTab === "history" ? "is-active" : ""}"
                @click=${() => { this.rightTab = "history"; }}
              >
                执行历史
                ${logs.length > 0
                  ? html`<span class="hb-tab__count">${logs.length}</span>`
                  : nothing}
              </button>
            </div>

            <div class="hb-tab-content">
              ${this.rightTab === "approvals"
                ? html`
                    ${approvals.length === 0
                      ? html`
                          <div class="hb-empty--sm">
                            <span>暂无待审批操作</span>
                            <span class="hb-empty-sub">AI 巡检时提出的高风险操作会在这里等待审批</span>
                          </div>
                        `
                      : html`
                          <div class="hb-approvals">
                            ${approvals.map(
                              (p) => html`
                                <div class="hb-approval">
                                  <div class="hb-approval__head">
                                    <span class="hb-approval__level hb-approval__level--${p.level.toLowerCase()}">${p.level}</span>
                                    <code class="hb-approval__tool">${p.toolName}</code>
                                    <span class="hb-approval__device">${p.deviceName || p.deviceId}</span>
                                  </div>
                                  <p class="hb-approval__summary">${p.summary}</p>
                                  <div class="hb-approval__meta">
                                    <span>原因: ${p.reason}</span>
                                    <span>风险: ${p.risk}</span>
                                  </div>
                                  <div class="hb-approval__actions">
                                    <button class="hb-btn hb-btn--approve" @click=${() => this.onApprove(p.proposalId)}>
                                      <svg viewBox="0 0 20 20" fill="currentColor" width="14" height="14">
                                        <path fill-rule="evenodd" d="M16.707 5.293a1 1 0 010 1.414l-8 8a1 1 0 01-1.414 0l-4-4a1 1 0 011.414-1.414L8 12.586l7.293-7.293a1 1 0 011.414 0z" clip-rule="evenodd"/>
                                      </svg>
                                      批准执行
                                    </button>
                                    <button class="hb-btn hb-btn--reject" @click=${() => this.onReject(p.proposalId)}>
                                      拒绝
                                    </button>
                                  </div>
                                </div>
                              `
                            )}
                          </div>
                        `}
                  `
                : html`
                    ${logs.length === 0
                      ? html`
                          <div class="hb-empty--sm">
                            <span>心跳未触发</span>
                            <span class="hb-empty-sub">下一个周期到达后将自动执行</span>
                          </div>
                        `
                      : html`
                          <div class="hb-timeline">
                            ${logs.map(
                              (log, i) => {
                                const ok = log.status === "success";
                                return html`
                                  <div class="hb-timeline__item ${ok ? "is-ok" : "is-err"}" style="--i: ${i}">
                                    <div class="hb-timeline__track">
                                      <div class="hb-timeline__dot ${ok ? "is-ok" : "is-err"}"></div>
                                      ${i < logs.length - 1
                                        ? html`<div class="hb-timeline__line"></div>`
                                        : nothing}
                                    </div>
                                    <details class="hb-timeline__details">
                                      <summary class="hb-timeline__summary">
                                        <div class="hb-timeline__content">
                                          <div class="hb-timeline__row">
                                            <span class="hb-timeline__time">
                                              ${new Date(log.timestamp).toLocaleString("zh-CN", {
                                                month: "2-digit",
                                                day: "2-digit",
                                                hour: "2-digit",
                                                minute: "2-digit",
                                                second: "2-digit",
                                              })}
                                            </span>
                                            <span class="hb-timeline__badge ${ok ? "is-ok" : "is-err"}">
                                              ${ok ? "成功" : "失败"}
                                            </span>
                                          </div>
                                          <div class="hb-timeline__meta">
                                            <span>${log.taskCount} 个任务</span>
                                            ${log.errorMessage
                                              ? html`<span class="hb-timeline__errmsg">${log.errorMessage}</span>`
                                              : nothing}
                                          </div>
                                        </div>
                                        <span class="hb-timeline__chevron">
                                          <svg viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="2" width="12" height="12">
                                            <path d="M4 6l4 4 4-4" stroke-linecap="round" stroke-linejoin="round"/>
                                          </svg>
                                        </span>
                                      </summary>
                                      <div class="hb-timeline__expanded">
                                        <div class="hb-timeline__kv">
                                          <span class="hb-timeline__kv-label">时间</span>
                                          <span>${new Date(log.timestamp).toLocaleString("zh-CN")}</span>
                                        </div>
                                        <div class="hb-timeline__kv">
                                          <span class="hb-timeline__kv-label">任务数</span>
                                          <span>${log.taskCount}</span>
                                        </div>
                                        ${log.result
                                          ? html`
                                              <div class="hb-timeline__report">
                                                <div class="hb-timeline__kv-label">巡检报告</div>
                                                <div class="markdown-body">${unsafeHTML(md(log.result))}</div>
                                              </div>`
                                          : log.errorMessage
                                            ? html`
                                                <div class="hb-timeline__report hb-timeline__report--err">
                                                  <div class="hb-timeline__kv-label">错误详情</div>
                                                  <pre>${log.errorMessage}</pre>
                                                </div>`
                                            : nothing}
                                      </div>
                                    </details>
                                  </div>
                                `;
                              }
                            )}
                          </div>
                        `}
                  `}
            </div>
          </div>
        </div>
      </div>
    `;
  }
}
