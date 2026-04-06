/**
 * Chat thread - scrollable message list
 */

import { LitElement, html, css } from 'lit'
import { customElement, property } from 'lit/decorators.js'
import { repeat } from 'lit/directives/repeat.js'
import type { ChatMessage } from '../../types/agent-types'
import { scheduleChatScroll, handleChatScroll, type ChatScrollHost } from '../../lib/chat-scroll'
import './message-group'
import './streaming-message'
import { hostStyles } from '../../styles/shared-host'

@customElement('chat-thread')
export class ChatThread extends LitElement implements ChatScrollHost {
  @property({ type: Array }) messages: ChatMessage[] = []
  @property({ type: String }) streamingContent = ''
  @property({ type: Boolean }) isStreaming = false

  chatScrollFrame: number | null = null
  chatScrollTimeout: number | null = null
  chatHasAutoScrolled = false
  chatUserNearBottom = true
  chatNewMessagesBelow = false

  static styles = [hostStyles, css`
    :host { display: block; flex: 1; overflow: hidden; }
    .chat-thread {
      height: 100%;
      overflow-y: auto;
      padding: 16px 24px;
      display: flex;
      flex-direction: column;
      gap: 16px;
    }
    @media (max-width: 768px) {
      .chat-thread { padding: 12px; gap: 12px; }
    }
  `]

  disconnectedCallback() {
    super.disconnectedCallback()
    if (this.chatScrollFrame != null) {
      cancelAnimationFrame(this.chatScrollFrame)
      this.chatScrollFrame = null
    }
    if (this.chatScrollTimeout != null) {
      clearTimeout(this.chatScrollTimeout)
      this.chatScrollTimeout = null
    }
  }

  updated(changed: Map<string, unknown>) {
    if (changed.has('messages') || changed.has('streamingContent')) {
      scheduleChatScroll(this, true)
    }
  }

  private _onScroll(e: Event) {
    handleChatScroll(this, e)
  }

  render() {
    return html`
      <div class="chat-thread" @scroll="${this._onScroll}" role="log" aria-live="polite">
        ${repeat(this.messages, m => m.id, msg =>
          html`<message-group .message="${msg}"></message-group>`
        )}
        ${this.isStreaming && this.streamingContent ? html`
          <streaming-message .content="${this.streamingContent}"></streaming-message>
        ` : ''}
      </div>
    `
  }
}
