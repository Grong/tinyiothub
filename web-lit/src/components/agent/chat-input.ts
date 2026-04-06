/**
 * Chat input - textarea with send/stop button
 */

import { LitElement, html} from 'lit'
import { customElement, property, state } from 'lit/decorators.js'

@customElement('chat-input')
export class ChatInput extends LitElement {
  createRenderRoot() { return this }
  @property({ type: Boolean }) isStreaming = false
  @state() private value = ''

  

  private _handleInput(e: Event) {
    const textarea = e.target as HTMLTextAreaElement
    this.value = textarea.value
    // Auto-resize
    textarea.style.height = 'auto'
    textarea.style.height = Math.min(textarea.scrollHeight, 150) + 'px'
  }

  private _handleKeydown(e: KeyboardEvent) {
    if (e.key === 'Enter' && !e.shiftKey) {
      e.preventDefault()
      if (this.isStreaming) {
        this._handleStop()
      } else {
        this._handleSend()
      }
    }
    if (e.key === 'Escape' && this.isStreaming) {
      this._handleStop()
    }
  }

  private _handleSend() {
    const trimmed = this.value.trim()
    if (!trimmed) return
    this.dispatchEvent(new CustomEvent('message-send', {
      detail: { message: trimmed },
      bubbles: true, composed: true,
    }))
    this.value = ''
    // Reset textarea height
    const textarea = this.querySelector('textarea')
    if (textarea) {
      textarea.value = ''
      textarea.style.height = 'auto'
    }
  }

  private _handleStop() {
    this.dispatchEvent(new CustomEvent('message-stop', {
      bubbles: true, composed: true,
    }))
  }

  render() {
    return html`
      <div class="input-area">
        <div class="input-row">
          <textarea
            .value="${this.value}"
            ?disabled="${this.isStreaming}"
            placeholder="询问设备状态、告警、数据..."
            aria-label="输入消息"
            rows="1"
            @input="${this._handleInput}"
            @keydown="${this._handleKeydown}"
          ></textarea>
          <button
            class="send-btn ${this.isStreaming ? 'stop' : 'send'}"
            @click="${this.isStreaming ? this._handleStop : this._handleSend}"
            ?disabled="${!this.isStreaming && !this.value.trim()}"
            aria-label="${this.isStreaming ? '停止' : '发送'}"
          >
            ${this.isStreaming ? html`
              <svg viewBox="0 0 24 24" fill="currentColor"><rect x="6" y="6" width="12" height="12" rx="2"/></svg>
            ` : html`
              <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><path d="M5 12h14M12 5l7 7-7 7"/></svg>
            `}
          </button>
        </div>
      </div>
    `
  }
}
