/**
 * Chat input - textarea with send/stop button
 */

import { LitElement, html, css } from 'lit'
import { customElement, property, state } from 'lit/decorators.js'

@customElement('chat-input')
export class ChatInput extends LitElement {
  @property({ type: Boolean }) isStreaming = false
  @state() private value = ''

  static styles = css`
    :host { display: block; flex-shrink: 0; }
    .input-area {
      padding: 12px 24px 16px;
      border-top: 1px solid var(--border, #e2e8f0);
      background: var(--bg, #fff);
    }
    .input-row {
      display: flex;
      gap: 8px;
      align-items: flex-end;
    }
    textarea {
      flex: 1;
      resize: none;
      border: 1px solid var(--border, #e2e8f0);
      border-radius: 12px;
      padding: 10px 14px;
      font-size: 0.875rem;
      font-family: inherit;
      line-height: 1.5;
      min-height: 42px;
      max-height: 150px;
      overflow-y: auto;
      background: var(--bg, #fff);
      color: var(--text, #1a1a1a);
      outline: none;
      transition: border-color 0.15s;
    }
    textarea:focus {
      border-color: var(--accent, #6366f1);
    }
    textarea:disabled {
      opacity: 0.5;
      cursor: not-allowed;
    }
    textarea::placeholder {
      color: var(--text-muted, #94a3b8);
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
      background: var(--accent, #6366f1);
      color: #fff;
    }
    .send-btn.send:hover {
      background: var(--accent-hover, #4f46e5);
    }
    .send-btn.send:disabled {
      opacity: 0.4;
      cursor: not-allowed;
    }
    .send-btn.stop {
      background: var(--danger, #ef4444);
      color: #fff;
    }
    .send-btn.stop:hover {
      background: #dc2626;
    }
    .send-btn svg {
      width: 18px;
      height: 18px;
    }
    @media (max-width: 768px) {
      .input-area { padding: 8px 12px 12px; }
    }
  `

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
