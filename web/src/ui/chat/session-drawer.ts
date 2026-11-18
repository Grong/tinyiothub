import { LitElement, css, html, nothing, type PropertyValues } from 'lit';
import { customElement, property, state } from 'lit/decorators.js';
import {
  deleteChatSession,
  listChatSessions,
  renameChatSession,
  type SessionSummary,
} from '../../api/assistant.js';

const GROUPS: Array<{ kind: string; title: string; hint: string }> = [
  { kind: 'chat', title: '对话', hint: '你和大脑的往来' },
  { kind: 'scheduled', title: '巡检', hint: '按计划自动执行' },
  { kind: 'event', title: '事件', hint: '由告警或设备触发' },
];

const KIND_LABEL: Record<string, string> = {
  chat: '对话',
  scheduled: '巡检',
  event: '事件',
};

@customElement('session-drawer')
export class SessionDrawer extends LitElement {
  static styles = css`
    :host {
      position: fixed;
      inset: 0;
      z-index: var(--z-modal, 400);
      display: block;
      pointer-events: none;
    }
    :host(:not([open])) {
      display: none;
    }
    .scrim {
      position: absolute;
      inset: 0;
      background: var(--overlay-backdrop);
      backdrop-filter: blur(3px);
      animation: scrim-in 160ms ease-out;
    }
    .panel {
      position: absolute;
      top: 0;
      bottom: 0;
      left: 0;
      display: flex;
      flex-direction: column;
      width: min(320px, calc(100vw - 48px));
      padding: 18px 16px 16px;
      border-right: 1px solid var(--border);
      background: var(--card);
      color: var(--text);
      box-shadow: var(--shadow-xl);
      pointer-events: auto;
      animation: panel-in 200ms cubic-bezier(0.16, 1, 0.3, 1);
    }
    .head {
      display: flex;
      align-items: center;
      gap: 10px;
      margin-bottom: 14px;
    }
    .title {
      flex: 1;
      font-family: var(--font-display);
      font-size: 17px;
      font-weight: 600;
      letter-spacing: 0.01em;
    }
    .close {
      display: grid;
      place-items: center;
      width: 28px;
      height: 28px;
      border: 1px solid transparent;
      border-radius: 6px;
      background: transparent;
      color: var(--asst-muted, #7c838a);
      cursor: pointer;
    }
    .close:hover {
      border-color: var(--asst-hairline, #282d33);
      color: var(--asst-ink, #e9e6df);
    }
    .new {
      display: flex;
      align-items: center;
      justify-content: space-between;
      width: 100%;
      min-height: 38px;
      margin-bottom: 16px;
      padding: 0 12px;
      border: 1px solid var(--asst-hairline, #282d33);
      border-radius: 7px;
      background: var(--bg-elevated);
      color: var(--text);
      font-family: var(--mono);
      font-size: 10.5px;
      letter-spacing: 0.06em;
      cursor: pointer;
      transition: border-color 120ms ease;
    }
    .new:hover {
      border-color: var(--asst-hairline-strong, #3a4047);
    }
    .new span:last-child {
      color: var(--asst-signal, #d9a441);
      font-size: 14px;
    }
    .list {
      flex: 1;
      min-height: 0;
      overflow-y: auto;
      display: flex;
      flex-direction: column;
      gap: 16px;
      margin: 0 -4px;
      padding: 0 4px;
      scrollbar-width: thin;
      scrollbar-color: var(--asst-hairline, #282d33) transparent;
    }
    .group__head {
      display: flex;
      align-items: baseline;
      gap: 8px;
      margin-bottom: 5px;
    }
    .group__title {
      color: var(--asst-muted, #7c838a);
      font-family: var(--asst-mono, monospace);
      font-size: 9.5px;
      letter-spacing: 0.16em;
      text-transform: uppercase;
    }
    .group__hint {
      color: var(--asst-muted, #7c838a);
      font-size: 9.5px;
      opacity: 0.75;
    }
    .drawer-row {
      display: flex;
      align-items: stretch;
      gap: 2px;
      border-radius: 6px;
      transition: background 100ms ease;
    }
    .drawer-row:hover,
    .drawer-row:focus-within {
      background: var(--asst-raised, #191d21);
    }
    .drawer-row--current {
      background: var(--asst-raised, #191d21);
      box-shadow: inset 2px 0 0 var(--asst-signal, #d9a441);
    }
    .drawer-item {
      flex: 1;
      min-width: 0;
      padding: 8px 9px;
      border: 0;
      border-radius: 6px;
      background: transparent;
      color: var(--asst-ink-dim, #a8ada9);
      text-align: left;
      cursor: pointer;
      font-size: 12.5px;
      line-height: 1.35;
    }
    .drawer-row--current .drawer-item {
      color: var(--asst-ink, #e9e6df);
    }
    .drawer-item__title {
      display: block;
      overflow: hidden;
      text-overflow: ellipsis;
      white-space: nowrap;
    }
    .drawer-item__meta {
      display: block;
      margin-top: 3px;
      color: var(--asst-muted, #7c838a);
      font-family: var(--asst-mono, monospace);
      font-size: 9.5px;
      letter-spacing: 0.02em;
    }
    .drawer-item__state {
      margin-left: 6px;
    }
    .drawer-item__state::before {
      content: "";
      display: inline-block;
      width: 4px;
      height: 4px;
      margin-right: 4px;
      border-radius: 50%;
      background: var(--asst-muted, #7c838a);
      vertical-align: 1px;
    }
    .drawer-item__state--active {
      color: var(--accent);
    }
    .drawer-item__state--active::before {
      background: var(--accent);
      box-shadow: 0 0 6px var(--accent-glow);
    }
    .drawer-actions {
      display: flex;
      align-items: center;
      gap: 2px;
      padding-right: 4px;
      opacity: 0;
      transition: opacity 100ms ease;
    }
    .drawer-row:hover .drawer-actions,
    .drawer-row:focus-within .drawer-actions {
      opacity: 1;
    }
    .drawer-action {
      border: 0;
      background: transparent;
      color: var(--asst-muted, #7c838a);
      font-family: var(--asst-mono, monospace);
      font-size: 9px;
      letter-spacing: 0.04em;
      cursor: pointer;
      padding: 3px;
    }
    .drawer-action:hover {
      color: var(--asst-ink, #e9e6df);
    }
    .empty {
      padding: 4px 2px;
      color: var(--asst-muted, #7c838a);
      font-size: 12px;
      line-height: 1.6;
    }
    .error {
      color: var(--asst-danger, #d9615b);
    }
    @keyframes panel-in {
      from {
        transform: translateX(-12px);
        opacity: 0.4;
      }
    }
    @keyframes scrim-in {
      from {
        opacity: 0;
      }
    }
    @media (max-width: 600px) {
      .panel {
        width: 100%;
        padding-top: max(18px, env(safe-area-inset-top));
      }
      .drawer-actions {
        opacity: 1;
      }
    }
    @media (prefers-reduced-motion: reduce) {
      .panel,
      .scrim {
        animation: none;
      }
    }
  `;

  @property() agentId = 'default';
  @property() current = '';
  @property({ type: Boolean, reflect: true }) open = false;

  @state() private sessions: SessionSummary[] = [];
  @state() private error = '';
  private refreshSeq = 0;
  private refreshTimer: number | null = null;

  connectedCallback(): void {
    super.connectedCallback();
    window.addEventListener('keydown', this.onKeydown);
    void this.refresh();
    this.refreshTimer = window.setInterval(() => {
      if (document.visibilityState === 'visible') void this.refresh();
    }, 5_000);
  }

  disconnectedCallback(): void {
    window.removeEventListener('keydown', this.onKeydown);
    if (this.refreshTimer !== null) window.clearInterval(this.refreshTimer);
    this.refreshTimer = null;
    super.disconnectedCallback();
  }

  protected updated(changed: PropertyValues): void {
    if (changed.has('open') && this.open) {
      void this.refresh();
      const close = this.shadowRoot?.querySelector<HTMLButtonElement>('.close');
      close?.focus({ preventScroll: true });
    }
  }

  private onKeydown = (event: KeyboardEvent): void => {
    if (event.key === 'Escape' && this.open) this.close();
  };

  private close(): void {
    this.dispatchEvent(new CustomEvent('drawer-close', { bubbles: true, composed: true }));
  }

  async refresh(): Promise<void> {
    const seq = ++this.refreshSeq;
    try {
      const sessions = await listChatSessions(this.agentId);
      if (seq !== this.refreshSeq) return;
      this.sessions = sessions;
      this.error = '';
      const activeSessions = sessions.filter((session) => session.status === 'active');
      this.dispatchEvent(
        new CustomEvent<{ active: number; sessions: SessionSummary[] }>('sessions-updated', {
          detail: { active: activeSessions.length, sessions: activeSessions },
          bubbles: true,
          composed: true,
        }),
      );
    } catch (error) {
      if (seq !== this.refreshSeq) return;
      this.error = (error as Error).message || '加载会话失败';
    }
  }

  private sessionTitle(session: SessionSummary): string {
    return session.label || session.sessionKey.split('/').pop() || session.sessionKey;
  }

  private select(sessionKey: string): void {
    this.dispatchEvent(
      new CustomEvent<string>('session-select', {
        detail: sessionKey,
        bubbles: true,
        composed: true,
      }),
    );
    this.close();
  }

  private async rename(sessionKey: string, event: Event): Promise<void> {
    event.stopPropagation();
    const label = window.prompt(
      '会话名称',
      this.sessions.find((item) => item.sessionKey === sessionKey)?.label ?? '',
    );
    if (!label?.trim()) return;
    try {
      await renameChatSession(sessionKey, label.trim());
      await this.refresh();
    } catch (error) {
      this.error = (error as Error).message || '重命名失败';
    }
  }

  private async removeSession(sessionKey: string, event: Event): Promise<void> {
    event.stopPropagation();
    if (!window.confirm('删除这个会话？历史消息会一起删除。')) return;
    try {
      await deleteChatSession(sessionKey);
      this.dispatchEvent(
        new CustomEvent<string>('session-deleted', {
          detail: sessionKey,
          bubbles: true,
          composed: true,
        }),
      );
      await this.refresh();
    } catch (error) {
      this.error = (error as Error).message || '删除失败';
    }
  }

  render() {
    if (!this.open) return nothing;
    return html`
      <div class="scrim" @click=${() => this.close()}></div>
      <aside class="panel" role="dialog" aria-modal="true" aria-label="会话">
        <div class="head">
          <span class="title">会话</span>
          <button class="close" aria-label="关闭会话列表" @click=${() => this.close()}>✕</button>
        </div>
        <button
          class="new"
          @click=${() => {
            this.dispatchEvent(new CustomEvent('session-new', { bubbles: true, composed: true }));
            this.close();
          }}
        >
          <span>新会话</span><span aria-hidden="true">＋</span>
        </button>
        ${this.error ? html`<div class="empty error">${this.error}</div>` : nothing}
        <div class="list">
          ${GROUPS.map((group) => {
            const items = this.sessions
              .filter((session) => session.kind === group.kind)
              .sort((a, b) => {
                const active = Number(b.status === 'active') - Number(a.status === 'active');
                return active !== 0 ? active : (b.updatedAt ?? 0) - (a.updatedAt ?? 0);
              });
            if (items.length === 0) return '';
            return html`
              <section class="group">
                <div class="group__head">
                  <span class="group__title">${group.title}</span>
                  <span class="group__hint">${group.hint}</span>
                </div>
                ${items.map(
                  (session) => html`
                    <div
                      class="drawer-row ${session.sessionKey === this.current ? 'drawer-row--current' : ''}"
                    >
                      <button
                        class="drawer-item"
                        aria-current=${session.sessionKey === this.current ? 'true' : 'false'}
                        @click=${() => this.select(session.sessionKey)}
                      >
                        <span class="drawer-item__title">${this.sessionTitle(session)}</span>
                        <span class="drawer-item__meta">
          ${KIND_LABEL[session.kind] ?? session.kind}
          <span class="drawer-item__state ${session.status === 'active' ? 'drawer-item__state--active' : ''}"
            >${session.status === 'active' ? '运行中' : '已结束'}</span
          >
        </span>
                      </button>
                      <span class="drawer-actions">
                        <button
                          class="drawer-action"
                          aria-label=${`改名 ${this.sessionTitle(session)}`}
                          @click=${(event: Event) => this.rename(session.sessionKey, event)}
                        >
                          改名
                        </button>
                        <button
                          class="drawer-action"
                          aria-label=${`删除 ${this.sessionTitle(session)}`}
                          @click=${(event: Event) => this.removeSession(session.sessionKey, event)}
                        >
                          删除
                        </button>
                      </span>
                    </div>
                  `,
                )}
              </section>
            `;
          })}
          ${this.sessions.length === 0
            ? html`<div class="empty">还没有会话。从一句「巡检当前设备」开始。</div>`
            : nothing}
        </div>
      </aside>
    `;
  }
}

declare global {
  interface HTMLElementTagNameMap {
    'session-drawer': SessionDrawer;
  }
}
