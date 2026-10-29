import { LitElement, html, type TemplateResult } from 'lit';
import { customElement, property } from 'lit/decorators.js';
import { createDialogFocusTrap, type DialogFocusTrap } from './runtime-dialog-focus.js';

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

  private readonly titleId = `runtime-confirm-dialog-title-${++dialogCount}`;
  private readonly bodyId = `runtime-confirm-dialog-body-${dialogCount}`;
  private trap: DialogFocusTrap | null = null;

  createRenderRoot() {
    return this;
  }

  connectedCallback(): void {
    super.connectedCallback();
    this.trap = createDialogFocusTrap(this, {
      onEscape: () => this.close(),
      focusables: () => this.focusables(),
      primary: () => this.primaryButton(),
    });
    this.trap.attach();
  }

  disconnectedCallback(): void {
    this.trap?.detach();
    this.trap = null;
    super.disconnectedCallback();
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
    this.dispatchEvent(new CustomEvent('confirm-primary', { bubbles: true, composed: true }));
  }

  private confirmDanger(): void {
    this.dispatchEvent(new CustomEvent('confirm-danger', { bubbles: true, composed: true }));
  }

  render() {
    return html`<div
      class="runtime-confirm-dialog__overlay"
      role="dialog"
      aria-modal="true"
      aria-labelledby=${this.titleId}
      aria-describedby=${this.bodyId}
    >
      <div class="runtime-confirm-dialog">
        <h2 id=${this.titleId}>${this.title}</h2>
        <div class="runtime-confirm-dialog__body" id=${this.bodyId}>${this.body}</div>
        <div class="runtime-confirm-dialog__actions">
          <button type="button" data-action="primary" @click=${this.close}>
            ${this.primaryLabel}
          </button>
          <button type="button" data-action="danger" @click=${this.confirmDanger}>
            ${this.dangerLabel}
          </button>
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
