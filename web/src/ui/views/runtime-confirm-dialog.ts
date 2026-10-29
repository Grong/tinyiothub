import { LitElement, html, type TemplateResult } from 'lit';
import { customElement, property } from 'lit/decorators.js';

let dialogCount = 0;

/**
 * Safe-default confirmation used by destructive Runtime actions.
 *
 * The primary action is the non-destructive one (`Keep running` / `Keep job`);
 * the danger action is only reachable through its own button.
 */
@customElement('runtime-confirm-dialog')
export class RuntimeConfirmDialog extends LitElement {
  @property() title = '';
  @property({ attribute: false }) body: string | TemplateResult = '';
  @property() primaryLabel = 'Keep running';
  @property() dangerLabel = 'Cancel task';
  @property({ type: Boolean }) busy = false;

  private readonly titleId = `runtime-confirm-dialog-title-${++dialogCount}`;
  private readonly bodyId = `runtime-confirm-dialog-body-${dialogCount}`;

  createRenderRoot() {
    return this;
  }

  protected firstUpdated(): void {
    this.primaryButton()?.focus();
  }

  private primaryButton(): HTMLButtonElement | null {
    return this.querySelector<HTMLButtonElement>('button[data-action="primary"]');
  }

  private focusables(): HTMLElement[] {
    return [...this.querySelectorAll<HTMLElement>('button:not([disabled])')];
  }

  private close(): void {
    if (this.busy) return;
    this.dispatchEvent(new CustomEvent('confirm-primary', { bubbles: true, composed: true }));
  }

  private confirmDanger(): void {
    if (this.busy) return;
    this.dispatchEvent(new CustomEvent('confirm-danger', { bubbles: true, composed: true }));
  }

  private onKeydown(event: KeyboardEvent): void {
    if (event.key === 'Escape') {
      event.preventDefault();
      event.stopPropagation();
      this.close();
      return;
    }
    if (event.key !== 'Tab') return;
    const focusables = this.focusables();
    if (!focusables.length) return;
    const first = focusables[0];
    const last = focusables[focusables.length - 1];
    const active = document.activeElement;
    if (event.shiftKey) {
      if (active === first || !this.contains(active)) {
        event.preventDefault();
        last.focus();
      }
      return;
    }
    if (active === last || !this.contains(active)) {
      event.preventDefault();
      first.focus();
    }
  }

  render() {
    const style = html`<style>
      runtime-confirm-dialog { display: block; }
      .runtime-confirm-dialog__overlay {
        position: fixed; inset: 0; z-index: 60; display: grid; place-items: center;
        padding: var(--space-4, 16px); background: rgba(2, 6, 23, 0.62);
      }
      .runtime-confirm-dialog {
        width: min(480px, 100%); padding: var(--space-4, 16px);
        border: 1px solid var(--glass-border, rgba(148, 163, 184, 0.3));
        border-radius: var(--radius-lg, 14px); background: var(--surface, #111827);
        box-shadow: var(--shadow-lg, 0 24px 60px rgba(0, 0, 0, 0.45));
      }
      .runtime-confirm-dialog h2 { margin: 0 0 var(--space-3, 12px); font-size: 18px; }
      .runtime-confirm-dialog__body { color: var(--text, #e5e7eb); }
      .runtime-confirm-dialog__body p { margin: 0 0 var(--space-2, 8px); }
      .runtime-confirm-dialog__actions { display: flex; flex-wrap: wrap; gap: var(--space-2, 8px); justify-content: flex-end; margin-top: var(--space-4, 16px); }
      .runtime-confirm-dialog button { min-height: 38px; padding: 0 14px; border-radius: var(--radius-md, 10px); border: 1px solid var(--glass-border, rgba(148, 163, 184, 0.3)); background: var(--bg-subtle, rgba(255, 255, 255, 0.06)); color: inherit; cursor: pointer; }
      .runtime-confirm-dialog button[data-action="primary"] { border-color: var(--accent, #38bdf8); font-weight: 600; }
      .runtime-confirm-dialog button[data-action="danger"] { border-color: var(--danger, #ef4444); background: var(--danger-subtle, rgba(239, 68, 68, 0.12)); color: var(--danger, #ef4444); }
      .runtime-confirm-dialog button:disabled { opacity: 0.55; cursor: default; }
      .runtime-confirm-dialog button:focus-visible { outline: 2px solid var(--accent-glow-strong, #38bdf8); outline-offset: 2px; }
    </style>`;
    return html`${style}<div class="runtime-confirm-dialog__overlay" role="dialog" aria-modal="true"
      aria-labelledby=${this.titleId} aria-describedby=${this.bodyId} @keydown=${this.onKeydown}>
      <div class="runtime-confirm-dialog">
        <h2 id=${this.titleId}>${this.title}</h2>
        <div class="runtime-confirm-dialog__body" id=${this.bodyId}>${this.body}</div>
        <div class="runtime-confirm-dialog__actions">
          <button type="button" data-action="primary" ?disabled=${this.busy} @click=${this.close}>${this.primaryLabel}</button>
          <button type="button" data-action="danger" ?disabled=${this.busy} @click=${this.confirmDanger}>${this.dangerLabel}</button>
        </div>
      </div>
    </div>`;
  }
}

declare global {
  interface HTMLElementTagNameMap {
    'runtime-confirm-dialog': RuntimeConfirmDialog;
  }
}
