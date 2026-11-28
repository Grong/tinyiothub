import { LitElement, css, html, type PropertyValues } from 'lit';
import { customElement, property, state } from 'lit/decorators.js';
import { answerQuestion, type QuestionAnswer } from '../../api/assistant.js';
import { ApiError } from '../../api/client.js';
import type { PendingQuestion } from './timeline-model.js';

@customElement('question-card')
export class QuestionCard extends LitElement {
  static styles = css`
    :host {
      display: block;
      width: 100%;
      max-width: 760px;
      margin: 0 auto 14px;
    }
    .card {
      max-height: min(40vh, 340px);
      overflow-y: auto;
      padding: 13px 14px;
      border: 1px solid var(--border);
      border-radius: var(--radius-md);
      background: var(--card);
    }
    .head {
      display: flex;
      align-items: center;
      gap: 7px;
      margin-bottom: 9px;
      color: var(--accent);
      font-family: var(--mono);
      font-size: 9.5px;
      letter-spacing: 0.14em;
      text-transform: uppercase;
    }
    .head::before {
      content: "";
      width: 5px;
      height: 5px;
      background: var(--accent);
    }
    .question {
      margin-bottom: 12px;
    }
    .question__header {
      margin-bottom: 2px;
      color: var(--muted);
      font-family: var(--mono);
      font-size: 9.5px;
      letter-spacing: 0.04em;
    }
    .question__title {
      margin-bottom: 7px;
      color: var(--text);
      font-size: 13px;
      line-height: 1.55;
    }
    .options {
      display: flex;
      flex-direction: column;
      align-items: stretch;
      gap: 7px;
    }
    .question-option {
      padding: 9px 12px;
      border: 1px solid var(--border);
      border-radius: var(--radius-md);
      background: transparent;
      color: var(--text);
      cursor: pointer;
      font-size: 12px;
      line-height: 1.5;
      text-align: left;
    }
    .question-option:hover {
      border-color: var(--asst-hairline-strong, #3a4047);
      color: var(--asst-ink, #e9e6df);
    }
    .question-option[aria-pressed='true'] {
      border-color: var(--accent);
      background: var(--accent-subtle);
      color: var(--text);
    }
    .question-option__desc {
      display: block;
      margin-top: 3px;
      font-size: 10.5px;
      line-height: 1.5;
      color: var(--muted);
    }
    .footer {
      display: flex;
      justify-content: flex-end;
    }
    .question-submit {
      padding: 5px 13px;
      border: 0;
      border-radius: 6px;
      background: var(--accent-gradient);
      color: var(--accent-foreground);
      cursor: pointer;
      font-family: var(--mono);
      font-size: 10.5px;
    }
    .question-submit:disabled {
      opacity: 0.45;
      cursor: default;
    }
    .error {
      color: var(--danger);
      font-size: 11px;
    }
  `;

  @property({ attribute: false }) sessionKey = '';
  @property({ attribute: false }) pending: PendingQuestion | null = null;

  @state() private selected: string[][] = [];
  @state() private submitting = false;
  @state() private submitted = false;
  @state() private closed = false;
  @state() private error = '';

  protected willUpdate(changed: PropertyValues): void {
    if (changed.has('pending') && this.pending) {
      this.selected = this.pending.questions.map(() => []);
      this.submitted = false;
      this.closed = false;
      this.error = '';
    }
  }

  private toggle(questionIndex: number, label: string, multiSelect: boolean): void {
    const next = this.selected.map((list) => [...list]);
    const current = next[questionIndex] ?? [];
    if (multiSelect) {
      next[questionIndex] = current.includes(label)
        ? current.filter((item) => item !== label)
        : [...current, label];
    } else {
      next[questionIndex] = [label];
    }
    this.selected = next;
  }

  private get complete(): boolean {
    if (!this.pending) return false;
    return (
      this.selected.length === this.pending.questions.length &&
      this.selected.every((list) => list.length > 0)
    );
  }

  private async submit(): Promise<void> {
    if (!this.pending || !this.complete || this.submitting) return;
    this.submitting = true;
    this.error = '';
    const answer: QuestionAnswer = {
      answers: this.selected.map((selected, questionIndex) => ({ questionIndex, selected })),
    };
    try {
      await answerQuestion(this.sessionKey, this.pending.questionId, answer);
      this.submitted = true;
      this.dispatchEvent(new CustomEvent('answered'));
    } catch (error) {
      if (error instanceof ApiError && (error.code === 404 || error.code === 409)) {
        this.closed = true;
        this.dispatchEvent(new CustomEvent('answered'));
      } else {
        this.error = (error as Error).message || '提交回答失败';
      }
    } finally {
      this.submitting = false;
    }
  }

  render() {
    if (!this.pending) return html``;
    return html`
      <section class="card" aria-label="需要你决定">
        <div class="head">需要你决定</div>
        ${this.pending.questions.map(
          (question, index) => html`
            <div class="question">
              <div class="question__header">${question.header}</div>
              <div class="question__title">${question.question}</div>
              <div class="options">
                ${question.options.map(
                  (option) => html`
                    <button
                      class="question-option"
                      aria-pressed=${String((this.selected[index] ?? []).includes(option.label))}
                      @click=${() => this.toggle(index, option.label, question.multiSelect)}
                    >
                      ${option.label}
                      ${
                        option.description
                          ? html`<span class="question-option__desc">${option.description}</span>`
                          : ''
                      }
                    </button>
                  `,
                )}
              </div>
            </div>
          `,
        )}
        ${this.error ? html`<div class="error">${this.error}</div>` : ''}
        <div class="footer">
          ${
            this.closed
              ? html`<span class="question-option__desc">问题已关闭</span>`
              : this.submitted
                ? html`<span class="question-option__desc">已提交，等待大脑继续</span>`
                : html`<button
                  class="question-submit"
                  ?disabled=${!this.complete || this.submitting}
                  @click=${() => this.submit()}
                >
                  ${this.submitting ? '提交中…' : '提交'}
                </button>`
          }
        </div>
      </section>
    `;
  }
}

declare global {
  interface HTMLElementTagNameMap {
    'question-card': QuestionCard;
  }
}
