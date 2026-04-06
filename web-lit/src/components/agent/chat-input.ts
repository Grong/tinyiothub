/**
 * Chat input - textarea with send/stop button
 */

import { LitElement, html, css } from 'lit'
import { customElement, property, state } from 'lit/decorators.js'
import { hostStyles } from '../../styles/shared-host'

@customElement('chat-input')
export class ChatInput extends LitElement {
  @property({ type: Boolean }) isStreaming = false
  @state() private value = ''

  static styles = [hostStyles, css`
    :host { display: block; flex-shrink: 0; }
    .input-area {
      padding: 12px 24px 16px;
      box-shadow: 0 -1px 0 var(--card-highlight);
      background: var(--chrome);
    }
    .input-row {
      display: flex;
      gap: 8px;
      align-items: flex-end;
    }
    textarea {
      flex: 1;
      resize: none;
      border: none;
      border-radius: 12px;
      padding: 10px 14px;
      font-size: 0.875rem;
      font-family: inherit;
      line-height: 1.5;
      min-height: 42px;
      max-height: 150px;
      overflow-y: auto;
      background: var(--card);
      color: var(--text);
      outline: none;
      box-shadow: var(--glass-shadow-sm);
      transition: box-shadow var(--duration-normal) var(--ease-out);
    }
    textarea:focus {
      box-shadow: var(--focus-ring);
    }
    textarea:disabled {
      opacity: 0.5;
      cursor: not-allowed;
    }
    textarea::placeholder {
      color: var(--text-muted);
    }
    .send-btn {
      width: 42px;
      height: 42px;
      border-radius: 50%;
      border: none;
      display: flex;
      align-items: center;
      justify-content: center;
      cursor: pointer;
      flex-shrink: 0;
      transition: background 0.15s;
    }
    .send-btn.send {
      background: var(--accent);
      color: var(--text-on-accent);
    }
    .send-btn.send:hover {
      background: var(--accent-hover);
    }
    .send-btn.send:disabled {
      opacity: 0.4;
      cursor: not-allowed;
    }
    .send-btn.stop {
      background: var(--danger);
      color: var(--text-on-accent);
    }
    .send-btn.stop:hover {
      background: var(--danger-hover);
    }
    .send-btn svg {
      width: 18px;
      height: 18px;
    }
    @media (max-width: 768px) {
      .input-area { padding: 8px 12px 12px; }
    }
  `]

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
    const textarea = this.shadowRoot?.querySelector('textarea')
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
