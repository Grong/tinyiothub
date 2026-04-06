/**
 * Streaming message component
 * Shows live-updating content with typing cursor
 */

import { LitElement, html} from 'lit'
import { customElement, property } from 'lit/decorators.js'
import { toSanitizedMarkdownHtml } from '../../lib/markdown'

@customElement('streaming-message')
export class StreamingMessage extends LitElement {
  createRenderRoot() { return this }
  @property({ type: String }) content = ''

  

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
