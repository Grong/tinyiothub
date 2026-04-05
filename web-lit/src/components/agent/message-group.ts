/**
 * Message group component
 * Renders a single chat message (user or assistant) with A2UI surfaces
 */

import { LitElement, html, css } from 'lit'
import { customElement, property } from 'lit/decorators.js'
import type { ChatMessage } from '../../types/agent-types'
import { toSanitizedMarkdownHtml } from '../../lib/markdown'
import '../a2ui/a2ui-surface'
import '../a2ui/a2ui-component'

@customElement('message-group')
export class MessageGroup extends LitElement {
  @property({ type: Object }) message: ChatMessage | null = null

  static styles = css`
    :host { display: block; }
    .group {
      display: flex;
      gap: 8px;
      max-width: 100%;
    }
    .group.user {
      flex-direction: row-reverse;
    }
    .avatar {
      width: 32px;
      height: 32px;
      border-radius: 50%;
      display: flex;
      align-items: center;
      justify-content: center;
      font-size: 0.75rem;
      font-weight: 600;
      flex-shrink: 0;
    }
    .avatar.user {
      background: var(--accent, #6366f1);
      color: #fff;
    }
    .avatar.assistant {
      background: var(--bg-elevated, #e2e8f0);
      color: var(--text, #1a1a1a);
    }
    .content {
      display: flex;
      flex-direction: column;
      gap: 4px;
      max-width: 72%;
    }
    .bubble {
      padding: 10px 14px;
      border-radius: 12px;
      font-size: 0.875rem;
      line-height: 1.6;
    }
    .bubble.user {
      background: var(--accent, #6366f1);
      color: #fff;
    }
    .bubble.assistant {
      background: var(--bg-elevated, #f8fafc);
      color: var(--text, #1a1a1a);
    }
    .bubble :deep(pre) {
      background: rgba(0,0,0,0.1);
      border-radius: 6px;
      padding: 8px 12px;
      overflow-x: auto;
      font-size: 0.8125rem;
      margin: 8px 0;
    }
    .bubble :deep(code) {
      font-family: 'SF Mono', Consolas, monospace;
      font-size: 0.8125rem;
    }
    .bubble :deep(p) { margin: 0 0 0.5em; }
    .bubble :deep(p:last-child) { margin-bottom: 0; }
    .bubble :deep(table) {
      border-collapse: collapse;
      width: 100%;
      margin: 8px 0;
    }
    .bubble :deep(th), .bubble :deep(td) {
      border: 1px solid var(--border, #e2e8f0);
      padding: 4px 8px;
      font-size: 0.8125rem;
    }
    .timestamp {
      font-size: 0.6875rem;
      color: var(--text-muted, #888);
    }
    .group.user .timestamp { text-align: right; }
    .surfaces {
      display: flex;
      flex-direction: column;
      gap: 8px;
      margin-top: 4px;
    }
  `

  private _formatTime(ts: number): string {
    return new Date(ts).toLocaleTimeString([], { hour: '2-digit', minute: '2-digit' })
  }

  render() {
    if (!this.message) return ''
    const msg = this.message
    const isUser = msg.role === 'user'

    return html`
      <div class="group ${msg.role}">
        <div class="avatar ${msg.role}">
          ${isUser ? '我' : 'AI'}
        </div>
        <div class="content">
          <div class="bubble ${msg.role}">
            ${isUser
              ? html`${msg.content}`
              : html`<div .innerHTML="${toSanitizedMarkdownHtml(msg.content)}"></div>`
            }
          </div>
          ${!isUser && msg.surfaces && msg.surfaces.size > 0 ? html`
            <div class="surfaces">
              ${Array.from(msg.surfaces.entries()).map(([surfaceId, state]) => html`
                <a2ui-surface
                  .surfaceId="${surfaceId}"
                  .title="${state.title || ''}"
                  .components="${state.components}"
                  .dataModel="${state.dataModel}"
                ></a2ui-surface>
              `)}
            </div>
          ` : ''}
          <div class="timestamp">${this._formatTime(msg.timestamp)}</div>
        </div>
      </div>
    `
  }
}
