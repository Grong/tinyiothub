/**
 * Streaming message component
 * Shows live-updating content with typing cursor
 */

import { LitElement, html, css } from 'lit'
import { customElement, property } from 'lit/decorators.js'
import { toSanitizedMarkdownHtml } from '../../lib/markdown'
import { hostStyles } from '../../styles/shared-host'

@customElement('streaming-message')
export class StreamingMessage extends LitElement {
  @property({ type: String }) content = ''

  static styles = [hostStyles, css`
    :host { display: block; }
    .bubble {
      background: var(--card);
      border-radius: 12px;
      padding: 12px 16px;
      max-width: 72%;
      box-shadow: var(--glass-shadow-sm);
    }
    .cursor {
      display: inline-block;
      width: 2px;
      height: 1em;
      background: var(--accent);
      animation: blink 1s step-end infinite;
      vertical-align: text-bottom;
      margin-left: 1px;
    }
    @keyframes blink {
      50% { opacity: 0; }
    }
    .indicator {
      font-size: 0.75rem;
      color: var(--text-muted, #888);
      margin-top: 4px;
    }
    /* Markdown styling */
    .content :deep(pre) {
      background: var(--bg-elevated);
      border-radius: 6px;
      padding: 8px 12px;
      overflow-x: auto;
      font-size: 0.8125rem;
    }
    .content :deep(code) {
      font-family: 'SF Mono', Consolas, monospace;
      font-size: 0.8125rem;
    }
    .content :deep(p) { margin: 0 0 0.5em; }
    .content :deep(p:last-child) { margin-bottom: 0; }
  `]

  render() {
    const htmlContent = this.content ? toSanitizedMarkdownHtml(this.content) : ''
    return html`
      <div class="bubble">
        <div class="content" .innerHTML="${htmlContent}"></div>
        <span class="cursor"></span>
        <div class="indicator">AI 正在输入...</div>
      </div>
    `
  }
}
