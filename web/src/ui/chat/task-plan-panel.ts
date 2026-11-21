import { LitElement, html, nothing } from 'lit';
import { customElement, property, state } from 'lit/decorators.js';
import type { TodoItem } from '../../api/assistant.js';
import '../../styles/components/task-plan-panel.css';

@customElement('task-plan-panel')
export class TaskPlanPanel extends LitElement {
  createRenderRoot() {
    return this;
  }

  @property({ attribute: false }) todos: TodoItem[] = [];
  @property({ type: Boolean }) running = true;
  @state() private expanded = true;

  private toggle(): void {
    this.expanded = !this.expanded;
  }

  render() {
    const total = this.todos.length;
    if (total === 0) return nothing;
    const active = this.todos.filter((todo) => todo.status === 'in_progress');
    const pending = this.todos.filter((todo) => todo.status === 'pending');
    const done = this.todos.filter((todo) => todo.status === 'completed');
    const summary =
      active.length > 0
        ? `任务 ${active.length} 进行中 · ${pending.length} 待处理`
        : done.length === total
          ? '任务全部完成'
          : this.running
            ? `任务 ${pending.length} 待处理`
            : `本轮结束 · ${pending.length} 项未完成`;
    const warning = !this.running && pending.length > 0;
    return html`<section class="task-plan" aria-label="本轮任务">
      <button
        class="task-plan__header"
        type="button"
        aria-expanded=${this.expanded}
        @click=${() => this.toggle()}
      >
        <span class="task-plan__kicker">任务</span>
        <span class="task-plan__summary" data-warning=${warning || nothing}>${summary}</span>
        <span class="task-plan__done">${done.length}/${total}</span>
        <span class="task-plan__chevron" aria-hidden="true">${this.expanded ? '⌄' : '›'}</span>
      </button>
      ${
        this.expanded
          ? html`<ol class="task-plan__list">
              ${this.todos.map(
                (todo) => html`<li class="task-plan__item task-plan__item--${todo.status}">
                  <span class="task-plan__mark" aria-hidden="true">
                    ${todo.status === 'completed' ? '✓' : todo.status === 'in_progress' ? '◌' : '·'}
                  </span>
                  <span class="task-plan__text">
                    ${todo.status === 'in_progress' ? todo.activeForm || todo.content : todo.content}
                  </span>
                  ${todo.status === 'in_progress'
                    ? html`<span class="task-plan__live">进行中</span>`
                    : nothing}
                </li>`,
              )}
            </ol>`
          : nothing
      }
    </section>`;
  }
}

declare global {
  interface HTMLElementTagNameMap {
    'task-plan-panel': TaskPlanPanel;
  }
}
