// AI 运维中心 — unified AI operations dashboard
//
// Replaces the old standalone heartbeat page and agents heartbeat tab.
// Sections: 今日摘要 + 实时 Feed | 待审批 + 执行历史 | 信任配置 + 知识/记忆

import { LitElement, html, nothing } from "lit";
import { customElement, state } from "lit/decorators.js";
import { unsafeHTML } from "lit/directives/unsafe-html.js";
import { marked } from "marked";
import DOMPurify from "dompurify";
import { apiGet, apiPost, apiPut } from "../../api/client.js";

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

interface ActionDetail {
  tool: string;
  deviceId: string;
  summary: string;
}

interface ProposalDetail {
  level: string;
  toolName: string;
  deviceId: string;
  deviceName: string;
  summary: string;
  reason: string;
  risk: string;
  status: string;
}

interface ExecutionRecord {
  timestamp: string;
  taskCount: number;
  status: string;
  errorMessage?: string;
  result?: string;
  autoExecuted: ActionDetail[];
  pendingProposals: ProposalDetail[];
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

interface AISummary {
  autoCount: number;
  pendingCount: number;
  errorCount: number;
}

// ── Component ──

@customElement("view-ai-ops")
export class ViewAiOps extends LitElement {
  @state() private selectedWsId = "";
  @state() private wsLoading = true;

  @state() private config: HeartbeatConfig | null = null;
  @state() private logs: ExecutionRecord[] = [];
  @state() private approvals: PendingProposal[] = [];
  @state() private loading = false;
  @state() private error: string | null = null;
  @state() private initialLoadDone = false;

  @state() private approvingId: string | null = null;
  @state() private saving = false;
  private pollTimer: ReturnType<typeof setInterval> | null = null;

  createRenderRoot() {
    return this;
  }

  connectedCallback(): void {
    super.connectedCallback();
    this.loadWorkspaces();
    // Auto-refresh every 30s for near-real-time updates
    this.pollTimer = setInterval(() => {
      if (this.selectedWsId) this.loadAll();
    }, 30_000);
  }

  disconnectedCallback(): void {
    super.disconnectedCallback();
    if (this.pollTimer) clearInterval(this.pollTimer);
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
    // Only show loading skeleton on initial load, not on poll refreshes
    if (!this.initialLoadDone) {
      this.loading = true;
    }
    this.error = null;
    try {
      const [cfgRes, logsRes, apprRes] = await Promise.all([
        apiGet<HeartbeatConfig>(`/workspaces/${this.selectedWsId}/heartbeat/config`),
        apiGet<{ logs: ExecutionRecord[] }>(`/workspaces/${this.selectedWsId}/heartbeat/logs`),
        apiGet<{ proposals: PendingProposal[] }>(`/workspaces/${this.selectedWsId}/heartbeat/approvals`),
      ]);
      if (cfgRes.result && typeof cfgRes.result.tasks === "string") {
        cfgRes.result.tasks = JSON.parse(cfgRes.result.tasks);
      }
      this.config = cfgRes.result || null;
      this.logs = logsRes.result?.logs ?? [];
      this.approvals = apprRes.result?.proposals ?? [];
      this.initialLoadDone = true;
    } catch (err) {
      this.error = String(err);
    } finally {
      this.loading = false;
    }
  }

  // ── Actions ──

  private async approveProposal(proposalId: string): Promise<void> {
    this.approvingId = proposalId;
    try {
      await apiPost(`/workspaces/${this.selectedWsId}/heartbeat/approvals/${proposalId}/approve`);
      this.approvals = this.approvals.filter(p => p.proposalId !== proposalId);
      // Refresh logs to show execution result
      await this.loadAll();
    } catch (err) {
      this.error = String(err);
    } finally {
      this.approvingId = null;
    }
  }

  private async rejectProposal(proposalId: string): Promise<void> {
    this.approvingId = proposalId;
    try {
      await apiPost(`/workspaces/${this.selectedWsId}/heartbeat/approvals/${proposalId}/reject`);
      this.approvals = this.approvals.filter(p => p.proposalId !== proposalId);
    } catch (err) {
      this.error = String(err);
    } finally {
      this.approvingId = null;
    }
  }

  private async toggleHeartbeat(e: Event): Promise<void> {
    const enabled = (e.target as HTMLInputElement).checked;
    this.saving = true;
    try {
      await apiPut(`/workspaces/${this.selectedWsId}/heartbeat/config`, { enabled });
      this.config = { ...this.config!, enabled };
    } catch (err) {
      this.error = String(err);
    } finally {
      this.saving = false;
    }
  }

  private async changeInterval(e: Event): Promise<void> {
    const intervalMinutes = Number((e.target as HTMLSelectElement).value);
    this.saving = true;
    try {
      await apiPut(`/workspaces/${this.selectedWsId}/heartbeat/config`, { intervalMinutes });
      this.config = { ...this.config!, intervalMinutes };
    } catch (err) {
      this.error = String(err);
    } finally {
      this.saving = false;
    }
  }

  private computeSummary(): AISummary {
    const autoCount = this.logs.filter(l => l.status === "success").length;
    const errorCount = this.logs.filter(l => l.status === "error").length;
    const pendingCount = this.approvals.length;
    return { autoCount, pendingCount, errorCount };
  }

  // ── Render helpers ──

  private renderSummaryCard() {
    const s = this.computeSummary();
    if (this.loading) {
      return html`<div class="ao-summary"><div class="ao-skeleton ao-skeleton--row"></div></div>`;
    }
    return html`
      <div class="ao-summary">
        <div class="ao-stat">
          <span class="ao-stat__num ao-stat__num--ok">${s.autoCount}</span>
          <span class="ao-stat__label">自动处理</span>
        </div>
        <div class="ao-stat">
          <span class="ao-stat__num ao-stat__num--warn">${s.pendingCount}</span>
          <span class="ao-stat__label">待确认</span>
        </div>
        <div class="ao-stat">
          <span class="ao-stat__num ao-stat__num--err">${s.errorCount}</span>
          <span class="ao-stat__label">异常</span>
        </div>
      </div>
    `;
  }

  // Normalize level to a safe CSS class suffix: "L2" → "l2", "L3" → "l3", fallback to "l2"
  private levelClass(level: string): string {
    const upper = level.trim().toUpperCase();
    if (upper === "L2" || upper === "L3" || upper === "L0" || upper === "L1") return upper.toLowerCase();
    // Detect level from embedded pattern like "⚠️⚠️ L2 ⚠️" or "L2-xxx"
    const m = level.match(/L([0-3])/i);
    if (m) return `l${m[1]}`;
    return "l2";
  }

  private renderApprovals() {
    const items = this.approvals;
    if (this.loading) {
      return html`<div class="ao-card"><div class="ao-card__title">待审批</div><div class="ao-skeleton ao-skeleton--card"></div></div>`;
    }
    if (items.length === 0) {
      return html`
        <div class="ao-card">
          <div class="ao-card__title">待审批</div>
          <div class="ao-empty">
            <p>暂无待审批项 — AI 正在自动处理低风险操作</p>
          </div>
        </div>
      `;
    }
    return html`
      <div class="ao-card">
        <div class="ao-card__title">待审批 (${items.length})</div>
        <div class="ao-proposal-list">
        ${items.map(p => html`
          <div class="ao-proposal">
            <div class="ao-proposal__head">
              <span class="ao-badge ao-badge--${this.levelClass(p.level)}">${this.levelClass(p.level).toUpperCase()}</span>
              <span class="ao-proposal__tool">${p.toolName}</span>
              <span class="ao-proposal__device">${p.deviceName || p.deviceId}</span>
            </div>
            <p class="ao-proposal__summary">${p.summary}</p>
            <div class="ao-proposal__meta">
              <span>原因: ${p.reason}</span>
              <span>风险: ${p.risk}</span>
            </div>
            <div class="ao-proposal__actions">
              <button class="ao-btn ao-btn--approve"
                      ?disabled=${this.approvingId === p.proposalId}
                      @click=${() => this.approveProposal(p.proposalId)}>
                ${this.approvingId === p.proposalId ? '执行中…' : '批准'}
              </button>
              <button class="ao-btn ao-btn--reject"
                      ?disabled=${this.approvingId === p.proposalId}
                      @click=${() => this.rejectProposal(p.proposalId)}>
                拒绝
              </button>
            </div>
          </div>
        `)}
        </div>
      </div>
    `;
  }

  private renderHistory() {
    const items = this.logs;
    if (this.loading) {
      return html`<div class="ao-card"><div class="ao-card__title">执行历史</div><div class="ao-skeleton ao-skeleton--timeline"></div></div>`;
    }
    if (items.length === 0) {
      return html`
        <div class="ao-card">
          <div class="ao-card__title">执行历史</div>
          <div class="ao-empty">
            <p>暂无执行记录</p>
            ${!this.config?.enabled ? html`<p class="ao-hint">心跳尚未启用，请确认配置</p>` : nothing}
          </div>
        </div>
      `;
    }
    return html`
      <div class="ao-card">
        <div class="ao-card__title">执行历史</div>
        <div class="ao-timeline">
          ${items.slice(0, 20).map(log => html`
            <div class="ao-timeline__item">
              <div class="ao-timeline__track">
                <span class="ao-dot ao-dot--${log.status === "error" ? "err" : "ok"}"></span>
                <span class="ao-timeline__line"></span>
              </div>
              <details class="ao-timeline__details">
                <summary class="ao-timeline__summary">
                  <div class="ao-timeline__content">
                    <div class="ao-timeline__row">
                      <span class="ao-timeline__time">${log.timestamp}</span>
                      <span class="ao-timeline__badge ${log.status === "error" ? "ao-timeline__badge--err" : "ao-timeline__badge--ok"}">${log.status === "error" ? "异常" : "完成"}</span>
                    </div>
                    <div class="ao-timeline__meta">
                      <span>${log.taskCount} 项任务</span>
                      ${log.autoExecuted.length > 0 ? html`<span class="ao-timeline__tag">${log.autoExecuted.length} 个动作</span>` : nothing}
                      ${log.pendingProposals.length > 0 ? html`<span class="ao-timeline__tag ao-timeline__tag--warn">${log.pendingProposals.length} 个提案</span>` : nothing}
                      ${log.errorMessage ? html`<span class="ao-timeline__err">${log.errorMessage.slice(0, 60)}</span>` : nothing}
                    </div>
                  </div>
                  <span class="ao-timeline__chevron">
                    <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><polyline points="6 9 12 15 18 9"/></svg>
                  </span>
                </summary>
                <div class="ao-timeline__body">
                  ${log.autoExecuted.length > 0 ? html`
                    <div class="ao-actions">
                      <div class="ao-actions__title">执行动作</div>
                      ${log.autoExecuted.map(a => html`
                        <div class="ao-action-item">
                          <span class="ao-action-item__tool">${a.tool}</span>
                          ${a.deviceId ? html`<span class="ao-action-item__device">${a.deviceId}</span>` : nothing}
                          <span class="ao-action-item__summary">${a.summary}</span>
                        </div>
                      `)}
                    </div>
                  ` : nothing}
                  ${log.pendingProposals.length > 0 ? html`
                    <div class="ao-actions">
                      <div class="ao-actions__title">待审批提案</div>
                      ${log.pendingProposals.map(p => html`
                        <div class="ao-action-item ao-action-item--proposal">
                          <span class="ao-badge ao-badge--${this.levelClass(p.level)}">${this.levelClass(p.level).toUpperCase()}</span>
                          <span class="ao-action-item__tool">${p.toolName}</span>
                          ${p.deviceName || p.deviceId ? html`<span class="ao-action-item__device">${p.deviceName || p.deviceId}</span>` : nothing}
                          <span class="ao-action-item__summary">${p.summary}</span>
                        </div>
                      `)}
                    </div>
                  ` : nothing}
                  ${log.result ? html`<div class="ao-ai-response">${unsafeHTML(md(log.result))}</div>` : nothing}
                  ${!log.result && log.errorMessage ? html`<div class="ao-ai-response">${unsafeHTML(md(log.errorMessage))}</div>` : nothing}
                  ${!log.result && !log.errorMessage && log.autoExecuted.length === 0 && log.pendingProposals.length === 0 ? html`<span class="ao-hint">无详情</span>` : nothing}
                </div>
              </details>
            </div>
          `)}
        </div>
      </div>
    `;
  }

  private renderTrustConfig() {
    const intervals = [5, 15, 30, 60];
    return html`
      <div class="ao-card">
        <div class="ao-card__title">信任配置</div>
        ${this.config?.enabled !== undefined ? html`
          <div class="ao-status-row">
            <label class="ao-toggle-label">
              <input
                type="checkbox"
                class="ao-toggle-input"
                .checked=${this.config.enabled}
                ?disabled=${this.saving}
                @change=${this.toggleHeartbeat}
              />
              <span class="ao-toggle-track">
                <span class="ao-toggle-thumb"></span>
              </span>
              <span class="ao-toggle-text">${this.config.enabled ? '运行中' : '已停止'}</span>
            </label>
          </div>
          <div class="ao-config-row">
            <span>巡检间隔</span>
            <select
              class="ao-interval-select"
              .value=${String(this.config.intervalMinutes)}
              ?disabled=${this.saving}
              @change=${this.changeInterval}
            >
              ${intervals.map(m => html`
                <option value=${m} ?selected=${m === this.config!.intervalMinutes}>${m} 分钟</option>
              `)}
            </select>
          </div>
          <div class="ao-tasks">
            <div class="ao-tasks__header">
              <span>巡检任务</span>
              <span class="ao-tasks__count">${this.config.tasks?.filter(t => !t.paused).length ?? 0} 项活跃</span>
            </div>
            <ul class="ao-tasks__list">
              ${this.config.tasks?.map(t => html`
                <li class="ao-tasks__item ${t.paused ? 'ao-tasks__item--paused' : ''}">
                  <span class="ao-tasks__priority ao-tasks__priority--${t.priority}">${t.priority}</span>
                  <span class="ao-tasks__text">${t.text}</span>
                  ${t.paused ? html`<span class="ao-tasks__paused-tag">暂停</span>` : nothing}
                </li>
              `) ?? nothing}
            </ul>
          </div>
        ` : html`<div class="ao-skeleton ao-skeleton--row"></div>`}
      </div>
    `;
  }

  private renderKnowledge() {
    return html`
      <div class="ao-card">
        <div class="ao-card__title">知识 / 记忆</div>
        <div class="ao-empty">
          <p>AI 尚未记录任何知识 — 开始与 AI 对话或等待首次巡检</p>
        </div>
      </div>
    `;
  }

  // ── Main render ──

  render() {
    if (this.wsLoading) {
      return html`<div class="ao-page"><div class="ao-skeleton ao-skeleton--page"></div></div>`;
    }
    if (this.error && !this.selectedWsId) {
      return html`<div class="ao-page"><div class="ao-error">${this.error}<br/><button class="ao-btn" @click=${this.loadWorkspaces}>重试</button></div></div>`;
    }

    return html`
      <div class="ao-page">
        <!-- Top row: summary stats -->
        <div class="ao-top-row">
          ${this.renderSummaryCard()}
        </div>

        <!-- Main grid: left (approvals+history) / right (trust+knowledge) -->
        <div class="ao-grid">
          <div class="ao-left">
            ${this.renderApprovals()}
            ${this.renderHistory()}
          </div>
          <div class="ao-right">
            ${this.renderTrustConfig()}
            ${this.renderKnowledge()}
          </div>
        </div>
      </div>
    `;
  }
}
