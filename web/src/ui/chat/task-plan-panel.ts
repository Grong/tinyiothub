import { LitElement, html, nothing, type PropertyValues } from 'lit';
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
  @property({ type: Boolean }) startCollapsed = false;
  @state() private expanded = true;

  protected willUpdate(changed: PropertyValues): void {
    if (!this.hasUpdated && changed.has('startCollapsed')) {
      this.expanded = !this.startCollapsed;
    }
  }

  private toggle(): void {
    this.expanded = !this.expanded;
  }

  render() {
    const total = this.todos.length;
    if (total === 0) return nothing;
    const active = this.todos.filter((todo) => todo.status === 'in_progress');
    const pending = this.todos.filter((todo) => todo.status === 'pending');
    const done = this.todos.filter((todo) => todo.status === 'completed');
    const summary = [
      done.length > 0 ? `已完成 ${done.length}` : '',
      active.length > 0 ? `${active.length} 进行中` : '',
      pending.length > 0 ? `${pending.length} 待处理` : '',
    ]
      .filter(Boolean)
      .join(' · ');
    const warning = !this.running && pending.length > 0;
    return html`<section class="task-plan" aria-label="本轮任务">
      <button
        class="task-plan__header"
        type="button"
        aria-expanded=${this.expanded}
        @click=${() => this.toggle()}
      >
        <span class="task-plan__kicker">任务</span>
        <span class="task-plan__summary" data-warning=${warning || nothing}>${summary || '暂无进行中的任务'}</span>
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
