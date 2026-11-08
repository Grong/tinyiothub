import { LitElement, css, html, type PropertyValues } from 'lit';
import { customElement, property, state } from 'lit/decorators.js';
import { answerQuestion, type QuestionAnswer } from '../../api/assistant.js';
import type { PendingQuestion } from './timeline-model.js';

@customElement('question-card')
export class QuestionCard extends LitElement {
  static styles = css`
    :host {
      display: block;
      width: 100%;
      max-width: 760px;
      margin: 0 auto 10px;
    }
    .card {
      border: 1px solid var(--accent);
      border-radius: var(--radius-md);
      background: var(--card);
      padding: 12px;
    }
    .head {
      font-size: 12px;
      color: var(--accent);
      margin-bottom: 8px;
    }
    .question {
      margin-bottom: 12px;
    }
    .question__header {
      font-size: 11px;
      color: var(--muted);
      margin-bottom: 2px;
      letter-spacing: 0.02em;
    }
    .question__title {
      font-size: 13.5px;
      margin-bottom: 6px;
    }
    .options {
      display: flex;
      flex-wrap: wrap;
      gap: 6px;
    }
    .question-option {
      border: 1px solid var(--border);
      background: var(--panel-strong);
      color: var(--text);
      border-radius: 999px;
      padding: 6px 12px;
      cursor: pointer;
      font-size: 12.5px;
    }
    .question-option[aria-pressed='true'] {
      border-color: var(--accent);
      color: var(--accent);
    }
    .question-option__desc {
      display: block;
      font-size: 11px;
      color: var(--muted);
    }
    .footer {
      display: flex;
      justify-content: flex-end;
    }
    .question-submit {
      background: var(--accent);
      color: #fff;
      border: 0;
      border-radius: var(--radius-sm);
      padding: 6px 14px;
      cursor: pointer;
    }
    .question-submit:disabled {
      opacity: 0.5;
      cursor: default;
    }
    .error {
      color: var(--danger, #ef4444);
      font-size: 12px;
    }
  `;

  @property({ attribute: false }) sessionKey = '';
  @property({ attribute: false }) pending: PendingQuestion | null = null;

  @state() private selected: string[][] = [];
  @state() private submitting = false;
  @state() private submitted = false;
  @state() private error = '';

  protected willUpdate(changed: PropertyValues): void {
    if (changed.has('pending') && this.pending) {
      this.selected = this.pending.questions.map(() => []);
      this.submitted = false;
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
      this.error = (error as Error).message || '提交回答失败';
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
            this.submitted
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
