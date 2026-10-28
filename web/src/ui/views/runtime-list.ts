import { LitElement, html, type PropertyValues } from 'lit';
import { customElement, property, state } from 'lit/decorators.js';
import { ApiError, getWorkspaceId } from '../../api/client.js';
import { listRuntimeTasks } from '../../api/runtime.js';
import type { RuntimeTask } from '../../types/runtime.js';
import { groupRuntimeTasks } from './runtime-projection.js';
import { switchTaskUrl } from './runtime-url-state.js';

@customElement('view-runtime-list')
export class RuntimeListView extends LitElement {
  @property() workspaceId = getWorkspaceId() ?? '';
  @state() private tasks: RuntimeTask[] = [];
  @state() private connection: 'connecting' | 'online' | 'offline' = 'connecting';
  @state() private error = '';
  @state() private limit = 50;
  @state() private offset = 0;

  private generation = 0;
  private abort: AbortController | null = null;

  createRenderRoot() {
    return this;
  }

  connectedCallback(): void {
    super.connectedCallback();
    void this.load();
  }

  disconnectedCallback(): void {
    this.generation += 1;
    this.abort?.abort();
    super.disconnectedCallback();
  }

  willUpdate(changed: PropertyValues<this>): void {
    if (changed.has('workspaceId') && changed.get('workspaceId') !== undefined) {
      this.offset = 0;
      void this.load();
    }
  }

  private async load(): Promise<void> {
    const generation = ++this.generation;
    this.abort?.abort();
    this.abort = new AbortController();
    this.connection = 'connecting';
    this.error = '';
    try {
      const response = await listRuntimeTasks(
        this.workspaceId, this.limit, this.offset, this.abort.signal,
      );
      if (generation !== this.generation) return;
      this.tasks = response.result?.tasks ?? [];
      this.limit = response.result?.limit ?? this.limit;
      this.offset = response.result?.offset ?? this.offset;
      this.connection = 'online';
    } catch (error) {
      if (generation !== this.generation || this.abort.signal.aborted) return;
      this.connection = 'offline';
      this.error = error instanceof ApiError ? error.message : 'Network error loading Runtime tasks';
    }
  }

  private page(offset: number): void {
    this.offset = Math.max(0, offset);
    void this.load();
  }

  private renderRows(tasks: RuntimeTask[]) {
    return tasks.map((task) => html`
      <li style="padding: var(--space-3) 0; border-bottom: 1px solid var(--border);">
        <a href=${switchTaskUrl(task.id)} style="color: var(--text); text-decoration: none;">
          <strong>${task.objective}</strong>
        </a>
        <div style="color: var(--muted-foreground); font-size: 13px; margin-top: var(--space-1);">
          ${task.status} · <time datetime=${task.updatedAt}>${task.updatedAt}</time>
        </div>
      </li>
    `);
  }

  render() {
    const groups = groupRuntimeTasks(this.tasks);
    const label = this.connection === 'online' ? 'Online' : this.connection === 'offline' ? 'Offline' : 'Connecting';
    return html`
      <section style="max-width: 880px;" aria-labelledby="runtime-list-title">
        <header style="display: flex; align-items: center; gap: var(--space-3); margin-bottom: var(--space-4);">
          <h1 id="runtime-list-title" style="margin: 0; flex: 1;">Runtime</h1>
          <span role="status" style="color: var(--muted-foreground);">${label}</span>
          <a class="btn btn--primary" href="#/runtime/new" style="text-decoration: none;">New task</a>
        </header>
        ${this.error ? html`<p role="alert" style="color: var(--error);">${this.error}</p>` : ''}
        ${this.tasks.length === 0 && this.connection !== 'connecting'
          ? html`<p style="color: var(--muted-foreground);">No Runtime tasks</p>`
          : html`
              <h2>Current</h2>
              ${groups.current.length
                ? html`<ul style="list-style: none; padding: 0; margin: 0;">${this.renderRows(groups.current)}</ul>`
                : html`<p style="color: var(--muted-foreground);">No current tasks</p>`}
              <h2>Recent</h2>
              ${groups.recent.length
                ? html`<ul style="list-style: none; padding: 0; margin: 0;">${this.renderRows(groups.recent)}</ul>`
                : html`<p style="color: var(--muted-foreground);">No recent tasks</p>`}
            `}
        <footer style="display: flex; gap: var(--space-3); margin-top: var(--space-4);">
          <button class="btn runtime-previous" ?disabled=${this.offset === 0 || this.connection === 'connecting'}
            @click=${() => this.page(this.offset - this.limit)}>Previous</button>
          <button class="btn runtime-next" ?disabled=${this.tasks.length < this.limit || this.connection === 'connecting'}
            @click=${() => this.page(this.offset + this.limit)}>Next</button>
        </footer>
      </section>
    `;
  }
}

declare global {
  interface HTMLElementTagNameMap {
    'view-runtime-list': RuntimeListView;
  }
}
