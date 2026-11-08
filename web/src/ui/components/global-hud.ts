import { LitElement, css, html, nothing } from 'lit';
import { customElement, property, state } from 'lit/decorators.js';
import { fetchAssistantHud, type HudSnapshot } from '../../api/assistant.js';

const STATUS_LABEL: Record<string, string> = {
  idle: '空闲',
  working: '运行中',
  waiting_for_user: '等待你',
  paused: '已暂停',
  error: '异常',
};

function formatTime(value: string | null): string {
  if (!value) return '—';
  const date = new Date(value);
  return Number.isNaN(date.getTime())
    ? value
    : date.toLocaleTimeString([], { hour: '2-digit', minute: '2-digit' });
}

@customElement('global-hud')
export class GlobalHud extends LitElement {
  static styles = css`
    :host {
      position: absolute;
      top: 12px;
      left: 50%;
      transform: translateX(-50%);
      z-index: 20;
      width: min(560px, calc(100% - 32px));
    }
    .hud-card {
      display: flex;
      flex-direction: column;
      gap: 4px;
      width: 100%;
      text-align: left;
      border: 1px solid var(--border);
      background: var(--glass-chat-input-bg, rgba(24, 27, 34, 0.72));
      backdrop-filter: blur(18px);
      border-radius: var(--radius-lg);
      padding: 10px 12px;
      color: var(--text);
      cursor: pointer;
      box-shadow: var(--shadow-md, 0 8px 24px rgba(0, 0, 0, 0.28));
    }
    .hud-card:focus-visible {
      outline: 2px solid var(--accent);
      outline-offset: 2px;
    }
    .row {
      display: flex;
      align-items: center;
      gap: 10px;
      font-size: 12px;
    }
    .status {
      display: inline-flex;
      align-items: center;
      gap: 6px;
      font-weight: 600;
    }
    .dot {
      width: 7px;
      height: 7px;
      border-radius: 50%;
      background: var(--muted);
    }
    .dot--working {
      background: var(--accent);
    }
    .dot--waiting_for_user {
      background: var(--warn, #f59e0b);
    }
    .dot--error {
      background: var(--danger, #ef4444);
    }
    .dot--paused {
      background: var(--muted);
    }
    .muted {
      color: var(--muted);
    }
    .detail {
      margin-top: 6px;
      border-top: 1px solid var(--border);
      padding-top: 8px;
      display: grid;
      gap: 4px;
      font-size: 12px;
    }
    .detail__row {
      display: flex;
      justify-content: space-between;
      gap: 12px;
    }
    .detail__row span:first-child {
      color: var(--muted);
    }
    .events {
      margin: 4px 0 0;
      padding: 0;
      list-style: none;
      display: grid;
      gap: 2px;
      color: var(--muted);
    }
    .events li {
      overflow: hidden;
      text-overflow: ellipsis;
      white-space: nowrap;
    }
    .error {
      color: var(--danger, #ef4444);
    }
  `;

  @property() workspaceId = '';
  @property({ type: Number }) pollMs = 10_000;

  @state() private snapshot: HudSnapshot | null = null;
  @state() private expanded = false;
  @state() private error = '';

  private timer: number | null = null;

  connectedCallback(): void {
    super.connectedCallback();
    void this.refresh();
    if (this.pollMs > 0) {
      this.timer = window.setInterval(() => {
        if (document.visibilityState === 'visible') void this.refresh();
      }, this.pollMs);
    }
  }

  disconnectedCallback(): void {
    if (this.timer !== null) window.clearInterval(this.timer);
    super.disconnectedCallback();
  }

  async refresh(): Promise<void> {
    if (!this.workspaceId) return;
    try {
      this.snapshot = await fetchAssistantHud(this.workspaceId);
      this.error = '';
    } catch (error) {
      this.error = (error as Error).message || '加载全局状态失败';
    }
  }

  private toggle(event?: KeyboardEvent): void {
    if (event && event.key !== 'Enter' && event.key !== ' ') return;
    event?.preventDefault();
    this.expanded = !this.expanded;
  }

  render() {
    const snapshot = this.snapshot;
    if (!snapshot) {
      return html`<div
        class="hud-card"
        role="button"
        tabindex="0"
        @click=${() => this.toggle()}
        @keydown=${(event: KeyboardEvent) => this.toggle(event)}
      >
        <div class="row"><span class="muted">${this.error || '正在连接大脑…'}</span></div>
      </div>`;
    }
    const status = snapshot.assistant.status;
    return html`
      <div
        class="hud-card"
        role="button"
        tabindex="0"
        aria-expanded=${String(this.expanded)}
        @click=${() => this.toggle()}
        @keydown=${(event: KeyboardEvent) => this.toggle(event)}
      >
        <div class="row">
          <span class="status"
            ><span class="dot dot--${status}"></span>${STATUS_LABEL[status] ?? status}</span
          >
          <span class="muted">下次 ${formatTime(snapshot.schedule.nextWakeAt)}</span>
          <span class="muted"
            >设备 ${snapshot.devices.total} · 离线 ${snapshot.devices.offline}</span
          >
          <span class="muted">告警 ${snapshot.alarms.highPriority} 高</span>
          <span class="muted">${snapshot.pending.questions} 待你回答</span>
        </div>
        ${
          this.expanded
            ? html`<div class="detail">
                <div class="detail__row">
                  <span>Assistant</span><span>${STATUS_LABEL[status] ?? status}</span>
                </div>
                <div class="detail__row">
                  <span>调度</span
                  ><span
                    >${snapshot.schedule.enabled ? `每 ${Math.round(snapshot.schedule.intervalSeconds / 60)} 分钟` : '已关闭'}</span
                  >
                </div>
                <div class="detail__row">
                  <span>下次唤醒</span><span>${formatTime(snapshot.schedule.nextWakeAt)}</span>
                </div>
                <div class="detail__row">
                  <span>活动会话</span><span>${snapshot.activity.activeSessions}</span>
                </div>
                <div class="detail__row">
                  <span>待处理</span
                  ><span>${snapshot.pending.questions} 问 · ${snapshot.pending.approvals} 批</span>
                </div>
                <div class="detail__row">
                  <span>最近唤醒</span><span>${snapshot.schedule.lastOutcome ?? '—'}</span>
                </div>
                <ul class="events">
                  ${snapshot.recentEvents.slice(0, 5).map((event) => html`<li>${event.eventType}</li>`)}
                </ul>
              </div>`
            : nothing
        }
        ${this.error ? html`<div class="error">${this.error}</div>` : nothing}
      </div>
    `;
  }
}

declare global {
  interface HTMLElementTagNameMap {
    'global-hud': GlobalHud;
  }
}
