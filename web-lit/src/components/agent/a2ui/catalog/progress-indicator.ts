import { LitElement, html, css } from 'lit'
import { customElement, property } from 'lit/decorators.js'

@customElement('progress-indicator')
export class ProgressIndicator extends LitElement {
  @property({ type: Number }) value = 0
  @property({ type: String }) label = ''
  @property({ type: String }) status: 'running' | 'completed' | 'failed' | 'cancelled' = 'running'
  @property({ type: Number }) current = 0
  @property({ type: Number }) total = 0
  @property({ type: Boolean }) showCancel = false

  static styles = css`
    :host { display: block; }
    .container { padding: 8px 0; }
    .bar-bg {
      height: 6px;
      background: var(--border, #e2e8f0);
      border-radius: 3px;
      overflow: hidden;
    }
    .bar-fill {
      height: 100%;
      border-radius: 3px;
      transition: width 0.3s ease;
    }
    .bar-fill.running { background: var(--accent, #6366f1); }
    .bar-fill.completed { background: var(--ok, #22c55e); }
    .bar-fill.failed { background: var(--danger, #ef4444); }
    .bar-fill.cancelled { background: var(--text-muted, #94a3b8); }
    .info {
      display: flex;
      justify-content: space-between;
      align-items: center;
      margin-top: 6px;
      font-size: 0.75rem;
      color: var(--text-muted, #888);
    }
    .status-label { }
    .status-label.running { color: var(--accent, #6366f1); }
    .status-label.completed { color: var(--ok, #22c55e); }
    .status-label.failed { color: var(--danger, #ef4444); }
    .status-label.cancelled { color: var(--text-muted, #94a3b8); }
    .pct { font-family: monospace; }
    button {
      font-size: 0.75rem;
      padding: 2px 8px;
      border-radius: 4px;
      border: 1px solid var(--border, #e2e8f0);
      background: transparent;
      cursor: pointer;
      color: var(--danger, #ef4444);
    }
  `

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
