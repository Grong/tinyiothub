import { LitElement, css, html } from 'lit';
import { customElement, property, state } from 'lit/decorators.js';
import {
  deleteChatSession,
  listChatSessions,
  renameChatSession,
  type SessionSummary,
} from '../../api/assistant.js';

const GROUPS: Array<{ kind: string; title: string }> = [
  { kind: 'chat', title: '对话' },
  { kind: 'scheduled', title: '巡检' },
  { kind: 'event', title: '事件' },
];

@customElement('session-drawer')
export class SessionDrawer extends LitElement {
  static styles = css`
    :host {
      display: flex;
      flex-direction: column;
      min-height: 0;
      border-right: 1px solid var(--border);
      padding-right: var(--space-3);
    }
    .head {
      display: flex;
      align-items: center;
      gap: 8px;
      padding: 4px 2px 10px;
    }
    .title {
      font-size: 12px;
      color: var(--muted);
      flex: 1;
    }
    .new {
      border: 1px solid var(--border);
      background: var(--card);
      color: var(--text);
      border-radius: var(--radius-sm);
      padding: 4px 10px;
      cursor: pointer;
      font-size: 12px;
    }
    .list {
      overflow-y: auto;
      min-height: 0;
      display: flex;
      flex-direction: column;
      gap: 10px;
    }
    .group__title {
      font-size: 11px;
      color: var(--muted);
      margin: 4px 0;
      text-transform: uppercase;
      letter-spacing: 0.04em;
    }
    .drawer-row {
      display: flex;
      align-items: flex-start;
      gap: 4px;
      border-radius: var(--radius-sm);
    }
    .drawer-row--current {
      background: var(--card);
    }
    .drawer-row:hover {
      background: var(--card);
    }
    .drawer-item {
      flex: 1;
      min-width: 0;
      text-align: left;
      background: transparent;
      border: 0;
      border-radius: var(--radius-sm);
      padding: 6px 8px;
      color: var(--text);
      cursor: pointer;
      font-size: 13px;
    }
    .drawer-row--current .drawer-item {
      color: var(--accent);
    }
    .drawer-item__meta {
      display: block;
      font-size: 11px;
      color: var(--muted);
      margin-top: 2px;
    }
    .drawer-actions {
      display: flex;
      flex-direction: column;
      gap: 2px;
      padding: 4px 4px 0 0;
    }
    .drawer-action {
      border: 0;
      background: transparent;
      color: var(--muted);
      font-size: 11px;
      cursor: pointer;
      padding: 0 2px;
    }
    .drawer-action:hover {
      color: var(--text);
    }
    .empty {
      font-size: 12px;
      color: var(--muted);
      padding: 8px;
    }
  `;

  @property() agentId = 'default';
  @property() current = '';

  @state() private sessions: SessionSummary[] = [];
  @state() private error = '';

  connectedCallback(): void {
    super.connectedCallback();
    void this.refresh();
  }

  async refresh(): Promise<void> {
    try {
      this.sessions = await listChatSessions(this.agentId);
      this.error = '';
    } catch (error) {
      this.error = (error as Error).message || '加载会话失败';
    }
  }

  private select(sessionKey: string): void {
    this.dispatchEvent(
      new CustomEvent<string>('session-select', {
        detail: sessionKey,
        bubbles: true,
        composed: true,
      }),
    );
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
    return html`
      <div class="head">
        <span class="title">会话</span>
        <button
          class="new"
          @click=${() => this.dispatchEvent(new CustomEvent('session-new', { bubbles: true, composed: true }))}
        >
          ＋ 新会话
        </button>
      </div>
      ${this.error ? html`<div class="empty">${this.error}</div>` : ''}
      <div class="list">
        ${GROUPS.map((group) => {
          const items = this.sessions.filter((session) => session.kind === group.kind);
          if (items.length === 0) return '';
          return html`
            <section class="group">
              <div class="group__title">${group.title}</div>
              ${items.map(
                (session) => html`
                  <div
                    class="drawer-row ${session.sessionKey === this.current ? 'drawer-row--current' : ''}"
                  >
                    <button class="drawer-item" @click=${() => this.select(session.sessionKey)}>
                      ${session.label || session.sessionKey.split('/').pop() || session.sessionKey}
                      <span class="drawer-item__meta">
                        ${session.status === 'ended' ? '已结束' : '进行中'} · ${session.kind}
                      </span>
                    </button>
                    <span class="drawer-actions">
                      <button
                        class="drawer-action"
                        @click=${(event: Event) => this.rename(session.sessionKey, event)}
                      >
                        改名
                      </button>
                      <button
                        class="drawer-action"
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
        ${this.sessions.length === 0 ? html`<div class="empty">还没有会话</div>` : ''}
      </div>
    `;
  }
}

declare global {
  interface HTMLElementTagNameMap {
    'session-drawer': SessionDrawer;
  }
}
