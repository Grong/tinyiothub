import { LitElement, css, html, type PropertyValues } from 'lit';
import { customElement, property, state } from 'lit/decorators.js';
import { answerApproval } from '../../api/assistant.js';
import { ApiError } from '../../api/client.js';
import type { PendingApproval } from './timeline-model.js';
import { approvalCopy } from './approval-copy.js';

@customElement('approval-card')
export class ApprovalCard extends LitElement {
  static styles = css`
    :host {
      display: block;
      width: 100%;
      max-width: 760px;
      margin: 0 auto 14px;
    }
    .card {
      padding: 13px 14px;
      border: 1px solid var(--border);
      border-radius: var(--radius-md);
      background: var(--card);
    }
    .head {
      display: flex;
      align-items: center;
      gap: 7px;
      margin-bottom: 7px;
      color: var(--warn);
      font-family: var(--mono);
      font-size: 9.5px;
      letter-spacing: 0.14em;
      text-transform: uppercase;
    }
    .head::before {
      content: '';
      width: 5px;
      height: 5px;
      background: var(--warn);
    }
    .tool {
      font-family: var(--mono);
      font-size: 11.5px;
      color: var(--text);
    }
    .summary {
      margin-top: 7px;
      max-height: 132px;
      overflow: auto;
      color: var(--muted);
      font-size: 12px;
      line-height: 1.6;
      white-space: pre-wrap;
    }
    .actions {
      display: flex;
      justify-content: flex-end;
      gap: 7px;
      margin-top: 11px;
    }
    .approval-approve,
    .approval-deny {
      padding: 5px 13px;
      border-radius: 6px;
      cursor: pointer;
      font-family: var(--mono);
      font-size: 10.5px;
      letter-spacing: 0.03em;
    }
    .approval-approve {
      border: 0;
      background: var(--accent-gradient);
      color: var(--accent-foreground);
    }
    .approval-deny {
      border: 1px solid var(--border);
      background: transparent;
      color: var(--muted);
    }
    .approval-deny:hover {
      border-color: var(--asst-hairline-strong, #3a4047);
      color: var(--asst-ink, #e9e6df);
    }
    button:disabled {
      opacity: 0.45;
      cursor: default;
    }
    .done {
      color: var(--ok);
      font-family: var(--mono);
      font-size: 10px;
    }
    .error {
      margin-top: 6px;
      color: var(--danger);
      font-size: 11px;
    }
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
      this.dispatchEvent(new CustomEvent('answered', { detail: this.pending.approvalId }));
    } catch (error) {
      if (error instanceof ApiError && (error.code === 404 || error.code === 409)) {
        // The server no longer has a pending entry (for example a cancelled
        // turn or a restarted process). Close the stale card instead of
        // trapping the user behind an approval that can never be answered.
        this.done = true;
        this.dispatchEvent(new CustomEvent('answered', { detail: this.pending.approvalId }));
      } else {
        this.error = (error as Error).message || '提交审批失败';
      }
    } finally {
      this.submitting = false;
    }
  }

  render() {
    if (!this.pending) return html``;
    const copy = approvalCopy(this.pending);
    return html`
      <section class="card" aria-label="需要审批">
        <div class="head">需要你的批准</div>
        <div class="tool">${copy.title}</div>
        <div class="summary">${copy.description}</div>
        ${this.error ? html`<div class="error">${this.error}</div>` : ''}
        <div class="actions">
          ${
            this.done
              ? html`<span class="done">已处理，等待大脑继续</span>`
              : html`
                  <button
                    class="approval-deny"
                    ?disabled=${this.submitting}
                    @click=${() => this.decide('denied')}
                  >
                    拒绝
                  </button>
                  <button
                    class="approval-approve"
                    ?disabled=${this.submitting}
                    @click=${() => this.decide('approved')}
                  >
                    批准
                  </button>
                `
          }
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
