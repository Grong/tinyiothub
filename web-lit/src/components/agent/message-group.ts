/**
 * Message group component
 * Renders a single chat message (user or assistant) with A2UI surfaces
 */

import { LitElement, html} from 'lit'
import { customElement, property } from 'lit/decorators.js'
import type { ChatMessage } from '../../types/agent-types'
import { toSanitizedMarkdownHtml } from '../../lib/markdown'
import './a2ui/a2ui-surface'
import './a2ui/a2ui-component'

@customElement('message-group')
export class MessageGroup extends LitElement {
  createRenderRoot() { return this }
  @property({ type: Object }) message: ChatMessage | null = null

  

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
