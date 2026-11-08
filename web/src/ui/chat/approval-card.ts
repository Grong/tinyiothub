import { LitElement, css, html, type PropertyValues } from 'lit';
import { customElement, property, state } from 'lit/decorators.js';
import { answerApproval } from '../../api/assistant.js';
import type { PendingApproval } from './timeline-model.js';

@customElement('approval-card')
export class ApprovalCard extends LitElement {
  static styles = css`
    :host { display: block; width: 100%; max-width: 760px; margin: 0 auto 10px; }
    .card { border: 1px solid var(--warn, #f59e0b); border-radius: var(--radius-md); background: var(--card); padding: 12px; }
    .head { font-size: 12px; color: var(--warn, #f59e0b); margin-bottom: 6px; }
    .tool { font-family: var(--mono); font-size: 12.5px; color: var(--text); }
    .summary { margin-top: 6px; font-size: 12px; color: var(--muted); max-height: 120px; overflow: auto; white-space: pre-wrap; }
    .actions { display: flex; justify-content: flex-end; gap: 8px; margin-top: 10px; }
    .approval-approve, .approval-deny { border-radius: var(--radius-sm); padding: 6px 14px; cursor: pointer; font-size: 12.5px; }
    .approval-approve { background: var(--accent); color: #fff; border: 0; }
    .approval-deny { background: transparent; color: var(--text); border: 1px solid var(--border); }
    button:disabled { opacity: 0.5; cursor: default; }
    .done { font-size: 12px; color: var(--muted); }
    .error { color: var(--danger, #ef4444); font-size: 12px; margin-top: 6px; }
  `;

  @property({ attribute: false }) sessionKey = '';
  @property({ attribute: false }) pending: PendingApproval | null = null;

  @state() private submitting = false;
  @state() private done = false;
  @state() private error = '';

  protected willUpdate(changed: PropertyValues): void {
    if (changed.has('pending') && this.pending) {
      this.done = false;
      this.error = '';
    }
  }

  private async decide(decision: 'approved' | 'denied'): Promise<void> {
    if (!this.pending || this.submitting) return;
    this.submitting = true;
    this.error = '';
    try {
      await answerApproval(this.sessionKey, this.pending.approvalId, decision);
      this.done = true;
      this.dispatchEvent(new CustomEvent('answered'));
    } catch (error) {
      this.error = (error as Error).message || '提交审批失败';
    } finally {
      this.submitting = false;
    }
  }

  render() {
    if (!this.pending) return html``;
    return html`
      <section class="card" aria-label="需要审批">
        <div class="head">需要你的批准</div>
        <div class="tool">${this.pending.tool}</div>
        <div class="summary">${this.pending.summary}</div>
        ${this.error ? html`<div class="error">${this.error}</div>` : ''}
        <div class="actions">
          ${this.done
            ? html`<span class="done">已处理，等待大脑继续</span>`
            : html`
                <button class="approval-deny" ?disabled=${this.submitting} @click=${() => this.decide('denied')}>
                  拒绝
                </button>
                <button class="approval-approve" ?disabled=${this.submitting} @click=${() => this.decide('approved')}>
                  批准
                </button>
              `}
        </div>
      </section>
    `;
  }
}

declare global {
  interface HTMLElementTagNameMap {
    'approval-card': ApprovalCard;
  }
}
