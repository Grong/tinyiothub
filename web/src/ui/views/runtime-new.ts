import { LitElement, html } from 'lit';
import { customElement, property, state } from 'lit/decorators.js';
import { ApiError, getWorkspaceId } from '../../api/client.js';
import { createRuntimeTask, RuntimeCreateModel } from '../../api/runtime.js';
import { replaceRuntimeTask } from './runtime-url-state.js';

@customElement('view-runtime-new')
export class RuntimeNewView extends LitElement {
  @property() workspaceId = getWorkspaceId() ?? '';
  @state() private objective = '';
  @state() private pending = false;
  @state() private error = '';

  private model = new RuntimeCreateModel();

  createRenderRoot() {
    return this;
  }

  private async submit(event: Event): Promise<void> {
    event.preventDefault();
    if (this.pending) return;
    const objective = this.objective.trim();
    if (!objective) {
      this.error = 'Objective is required';
      return;
    }
    this.pending = true;
    this.error = '';
    try {
      const response = await createRuntimeTask(
        this.workspaceId,
        objective,
        this.model.keyFor(objective),
      );
      this.model.reset();
      if (!response.result) throw new ApiError(500, 'Task creation response missing');
      replaceRuntimeTask(response.result.taskId);
    } catch (error) {
      if (error instanceof ApiError) this.model.reset();
      this.error = error instanceof ApiError
        ? error.message
        : 'Network error; retry with the same objective';
    } finally {
      this.pending = false;
    }
  }

  private cancel(): void {
    window.location.hash = '#/runtime';
  }

  render() {
    return html`
      <section class="card" style="max-width: 720px; padding: var(--space-6);" aria-labelledby="runtime-new-title">
        <h1 id="runtime-new-title" style="margin-top: 0;">New Runtime task</h1>
        <p style="color: var(--muted-foreground);">
          A continuous Task can wait for device work, watches, and approvals while preserving its evidence trail.
        </p>
        <form @submit=${this.submit}>
          <label for="runtime-objective" style="display: block; margin-bottom: var(--space-2);">Objective</label>
          <textarea
            id="runtime-objective"
            rows="5"
            style="width: 100%; box-sizing: border-box; resize: vertical;"
            .value=${this.objective}
            @input=${(event: InputEvent) => {
              this.objective = (event.target as HTMLTextAreaElement).value;
            }}
          ></textarea>
          ${this.error ? html`<p role="alert" style="color: var(--error);">${this.error}</p>` : ''}
          <div style="display: flex; gap: var(--space-3); margin-top: var(--space-4);">
            <button class="btn btn--primary" type="submit" ?disabled=${this.pending}>
              ${this.pending ? 'Creating task' : 'Create task'}
            </button>
            <button class="btn" type="button" ?disabled=${this.pending} @click=${this.cancel}>Cancel</button>
          </div>
        </form>
      </section>
    `;
  }
}

declare global {
  interface HTMLElementTagNameMap {
    'view-runtime-new': RuntimeNewView;
  }
}
