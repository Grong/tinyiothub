import { LitElement, html} from 'lit'
import { customElement, property } from 'lit/decorators.js'

@customElement('progress-indicator')
export class ProgressIndicator extends LitElement {
  createRenderRoot() { return this }
  @property({ type: Number }) value = 0
  @property({ type: String }) label = ''
  @property({ type: String }) status: 'running' | 'completed' | 'failed' | 'cancelled' = 'running'
  @property({ type: Number }) current = 0
  @property({ type: Number }) total = 0
  @property({ type: Boolean }) showCancel = false

  

  private _statusLabels: Record<string, string> = {
    running: '进行中',
    completed: '已完成',
    failed: '失败',
    cancelled: '已取消',
  }

  private _handleCancel() {
    this.dispatchEvent(new CustomEvent('a2ui-action', {
      detail: { action: 'cancel' },
      bubbles: true, composed: true,
    }))
  }

  render() {
    const pct = Math.min(100, Math.max(0, this.value))
    return html`
      <div class="container">
        ${this.label ? html`<div style="font-size: 0.8125rem; margin-bottom: 4px;">${this.label}</div>` : ''}
        <div class="bar-bg">
          <div class="bar-fill ${this.status}" style="width: ${pct}%"></div>
        </div>
        <div class="info">
          <span class="status-label ${this.status}">${this._statusLabels[this.status] || this.status}</span>
          ${this.total > 0 ? html`<span>${this.current} / ${this.total}</span>` : ''}
          <span class="pct">${pct}%</span>
          ${this.showCancel && this.status === 'running' ? html`<button @click="${this._handleCancel}">取消</button>` : ''}
        </div>
      </div>
    `
  }
}
