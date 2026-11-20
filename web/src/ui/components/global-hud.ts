import { LitElement, css, html, nothing } from 'lit';
import { customElement, property, state } from 'lit/decorators.js';
import { fetchAssistantHud, type HudSnapshot } from '../../api/assistant.js';

const STATUS_LABEL: Record<string, string> = {
  idle: '待命',
  working: '运行中',
  waiting_for_user: '等待你',
  paused: '已暂停',
  error: '异常',
};

const EVENT_LABEL: Record<string, string> = {
  'turn/start': '开始处理',
  'turn/end': '本轮结束',
  'tool/call': '调用工具',
  'tool/result': '工具返回',
  'skill/loaded': '载入技能',
  'todo/write': '计划更新',
  'question/asked': '向你提问',
  'approval/requested': '请求审批',
  'approval/resolved': '审批完成',
  'subagent/started': '派出子代理',
  'subagent/completed': '子代理返回',
};

function formatTime(value: string | null): string {
  if (!value) return '—';
  const date = new Date(value);
  return Number.isNaN(date.getTime())
    ? value
    : date.toLocaleTimeString([], { hour: '2-digit', minute: '2-digit', hour12: false });
}

const OUTCOME_LABEL: Record<string, string> = {
  completed: '完成',
  failed: '失败',
  cancelled: '已取消',
  skipped: '已跳过',
  unknown: '未知',
};

function outcomeLabel(value: string | null): string {
  if (!value) return '—';
  return OUTCOME_LABEL[value] ?? value;
}

function eventLabel(eventType: string): string {
  return EVENT_LABEL[eventType] ?? eventType;
}

@customElement('global-hud')
export class GlobalHud extends LitElement {
  static styles = css`
    :host {
      display: block;
    }
    .hud-rail {
      border: 1px solid var(--glass-border);
      border-radius: var(--radius-md);
      background: var(--glass-bg);
      box-shadow: var(--glass-shadow);
      backdrop-filter: var(--glass-blur);
      -webkit-backdrop-filter: var(--glass-blur);
      overflow: hidden;
      transition: border-color 120ms ease;
    }
    .hud-rail[data-status="working"],
    .hud-rail[data-status="waiting_for_user"] {
      border-color: color-mix(in srgb, var(--accent) 55%, var(--glass-border));
    }
    .hud-card {
      display: flex;
      align-items: stretch;
      width: 100%;
      min-height: 46px;
      padding: 0;
      border: 0;
      background: transparent;
      color: var(--asst-ink, #e9e6df);
      text-align: left;
      cursor: pointer;
      font: inherit;
    }
    .hud-card:focus-visible {
      outline: 1px solid var(--asst-signal, #d9a441);
      outline-offset: -2px;
    }
    .status {
      display: flex;
      align-items: center;
      gap: 9px;
      flex: none;
      padding: 0 14px;
      border-right: 1px solid var(--asst-hairline, #282d33);
    }
    .beacon {
      position: relative;
      width: 8px;
      height: 8px;
      border-radius: 50%;
      background: var(--muted);
      flex: none;
    }
    .beacon--working,
    .beacon--waiting_for_user {
      background: var(--accent);
      box-shadow: 0 0 8px var(--accent-glow-strong);
    }
    .beacon--working::after,
    .beacon--waiting_for_user::after {
      content: "";
      position: absolute;
      inset: -3px;
      border: 1px solid var(--accent);
      border-radius: 50%;
      box-shadow: 0 0 10px var(--accent-glow);
      animation: hud-pulse 2s ease-out infinite;
    }
    .beacon--waiting_for_user::after {
      animation-duration: 3.2s;
    }
    .beacon--error {
      background: var(--danger);
    }
    .status__text {
      display: flex;
      flex-direction: column;
      gap: 1px;
    }
    .status__label {
      font-family: var(--asst-mono, monospace);
      font-size: 10px;
      letter-spacing: 0.16em;
      text-transform: uppercase;
      color: var(--asst-muted, #7c838a);
    }
    .status__value {
      font-family: var(--asst-display, sans-serif);
      font-size: 15px;
      font-weight: 600;
      letter-spacing: 0.01em;
      color: var(--asst-ink, #e9e6df);
      white-space: nowrap;
    }
    .hud-rail[data-status="working"] .status__value,
    .hud-rail[data-status="waiting_for_user"] .status__value {
      color: var(--asst-signal, #d9a441);
    }
    .readouts {
      display: flex;
      align-items: center;
      flex: 1;
      min-width: 0;
      overflow: hidden;
      padding: 0 14px;
    }
    .readout {
      display: flex;
      align-items: baseline;
      gap: 5px;
      padding: 0 13px;
      border-right: 1px solid var(--asst-hairline, #282d33);
      white-space: nowrap;
    }
    .readout:first-child {
      padding-left: 0;
    }
    .readout:last-child {
      border-right: 0;
    }
    .readout b {
      font-family: var(--asst-display, sans-serif);
      font-size: 15px;
      font-weight: 600;
      color: var(--asst-ink, #e9e6df);
    }
    .readout span {
      font-family: var(--asst-mono, monospace);
      font-size: 9.5px;
      letter-spacing: 0.05em;
      color: var(--asst-muted, #7c838a);
    }
    .readout--alert b {
      color: var(--accent);
    }
    .readout--warn b {
      color: var(--warn);
    }
    .readout--quiet b {
      color: var(--asst-ink-dim, #a8ada9);
    }
    .schedule {
      display: flex;
      align-items: center;
      gap: 8px;
      flex: none;
      padding: 0 14px;
      border-left: 1px solid var(--asst-hairline, #282d33);
      font-family: var(--asst-mono, monospace);
      font-size: 9.5px;
      letter-spacing: 0.05em;
      color: var(--asst-muted, #7c838a);
      white-space: nowrap;
    }
    .schedule b {
      color: var(--asst-ink-dim, #a8ada9);
      font-weight: 500;
    }
    .chevron {
      font-size: 9px;
      color: var(--asst-muted, #7c838a);
      transition: transform 140ms ease;
    }
    .hud-card[aria-expanded="true"] .chevron {
      transform: rotate(180deg);
    }
    .detail {
      display: grid;
      grid-template-columns: repeat(2, minmax(0, 1fr));
      gap: 0 24px;
      padding: 10px 14px 12px;
      border-top: 1px solid var(--asst-hairline, #282d33);
      background: var(--bg-accent);
    }
    .detail__row {
      display: flex;
      align-items: baseline;
      justify-content: space-between;
      gap: 12px;
      padding: 4px 0;
      border-bottom: 1px dotted var(--asst-hairline, #282d33);
      font-family: var(--asst-mono, monospace);
      font-size: 10.5px;
      color: var(--asst-ink-dim, #a8ada9);
    }
    .detail__row span:first-child {
      color: var(--asst-muted, #7c838a);
    }
    .events {
      grid-column: 1 / -1;
      margin: 8px 0 0;
      padding: 0;
      list-style: none;
      display: flex;
      flex-wrap: wrap;
      gap: 5px;
    }
    .events li {
      padding: 2px 7px;
      border: 1px solid var(--asst-hairline, #282d33);
      border-radius: 999px;
      font-family: var(--asst-mono, monospace);
      font-size: 9.5px;
      color: var(--asst-muted, #7c838a);
      overflow: hidden;
      text-overflow: ellipsis;
      white-space: nowrap;
      max-width: 100%;
    }
    .error {
      padding: 0 14px 8px;
      color: var(--asst-danger, #d9615b);
      font-family: var(--asst-mono, monospace);
      font-size: 10px;
    }
    .loading {
      padding: 12px 14px;
      color: var(--asst-muted, #7c838a);
      font-family: var(--asst-mono, monospace);
      font-size: 10.5px;
    }
    @keyframes hud-pulse {
      0% {
        opacity: 0.7;
        transform: scale(0.7);
      }
      100% {
        opacity: 0;
        transform: scale(1.9);
      }
    }
    @media (max-width: 720px) {
      .hud-card {
        flex-wrap: wrap;
        min-height: 0;
      }
      .status {
        flex: 1;
        padding: 8px 12px;
        border-right: 0;
      }
      .schedule {
        margin-left: auto;
        padding: 8px 12px;
        border-left: 0;
      }
      .readouts {
        order: 3;
        flex: 0 0 100%;
        width: 100%;
        padding: 6px 0 7px;
        border-top: 1px solid var(--asst-hairline, #282d33);
        overflow: visible;
      }
      .readout {
        flex: 1;
        justify-content: center;
        gap: 3px;
        padding: 0 3px;
      }
      .readout b {
        font-size: 13px;
      }
      .readout span {
        font-size: 8.5px;
        letter-spacing: 0.03em;
      }
      .readout:first-child {
        display: none;
      }
      .readout:nth-child(2) {
        padding-left: 12px;
      }
      .readout:last-child {
        justify-content: flex-end;
        padding-right: 12px;
      }
    }
    @media (prefers-reduced-motion: reduce) {
      .beacon::after {
        animation: none;
      }
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
      this.dispatchEvent(
        new CustomEvent<HudSnapshot>('hud-update', {
          detail: this.snapshot,
          bubbles: true,
          composed: true,
        }),
      );
    } catch (error) {
      this.error = (error as Error).message || '加载全局状态失败';
    }
  }

  private toggle(): void {
    this.expanded = !this.expanded;
  }

  private statusText(status: string): string {
    return STATUS_LABEL[status] ?? status;
  }

  render() {
    const snapshot = this.snapshot;
    if (!snapshot) {
      return html`<div class="hud-rail">
        <div class="loading">${this.error || '正在连接大脑…'}</div>
      </div>`;
    }
    const status = snapshot.assistant.status;
    const pending = snapshot.pending.questions + snapshot.pending.approvals;
    const scheduleText = !snapshot.schedule.enabled
      ? html`巡检已暂停`
      : snapshot.schedule.nextWakeAt
        ? html`下次 <b>${formatTime(snapshot.schedule.nextWakeAt)}</b>`
        : html`巡检待命`;
    return html`
      <section class="hud-rail" data-status=${status} aria-label="全局状态">
        <button
          class="hud-card"
          aria-expanded=${String(this.expanded)}
          aria-label="展开全局状态"
          @click=${() => this.toggle()}
        >
          <span class="status">
            <span class="beacon beacon--${status}" aria-hidden="true"></span>
            <span class="status__text">
              <span class="status__label">大脑</span>
              <span class="status__value">${this.statusText(status)}</span>
            </span>
          </span>
          <span class="readouts">
            <span class="readout"><b>${snapshot.devices.total}</b><span>设备</span></span>
            <span class="readout ${snapshot.devices.offline > 0 ? 'readout--warn' : 'readout--quiet'}">
              <b>${snapshot.devices.offline}</b><span>离线</span>
            </span>
            <span class="readout ${snapshot.alarms.highPriority > 0 ? 'readout--warn' : 'readout--quiet'}">
              <b>${snapshot.alarms.highPriority}</b><span>高优告警</span>
            </span>
            <span class="readout ${pending > 0 ? 'readout--alert' : 'readout--quiet'}">
              <b>${pending}</b><span>待你</span>
            </span>
          </span>
          <span class="schedule">${scheduleText}<span class="chevron" aria-hidden="true">▾</span></span>
        </button>
        ${this.expanded
          ? html`<div class="detail">
              <div class="detail__row"><span>调度</span><span>${snapshot.schedule.enabled ? `每 ${Math.round(snapshot.schedule.intervalSeconds / 60)} 分钟` : '已关闭'}</span></div>
              <div class="detail__row"><span>下次唤醒</span><span>${formatTime(snapshot.schedule.nextWakeAt)}</span></div>
              <div class="detail__row"><span>活动会话</span><span>${snapshot.activity.activeSessions}</span></div>
              <div class="detail__row"><span>待处理</span><span>${snapshot.pending.questions} 问 · ${snapshot.pending.approvals} 批</span></div>
              <div class="detail__row"><span>最近唤醒</span><span>${outcomeLabel(snapshot.schedule.lastOutcome)}</span></div>
              <div class="detail__row"><span>Assistant 结论</span><span>${outcomeLabel(snapshot.assistant.lastOutcome)}</span></div>
              <ul class="events">
                ${snapshot.recentEvents.slice(0, 6).map((event) => html`<li>${eventLabel(event.eventType)}</li>`)}
              </ul>
            </div>`
          : nothing}
        ${this.error ? html`<div class="error">${this.error}</div>` : nothing}
      </section>
    `;
  }
}

declare global {
  interface HTMLElementTagNameMap {
    'global-hud': GlobalHud;
  }
}
