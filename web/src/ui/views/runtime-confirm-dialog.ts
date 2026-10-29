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

  private readonly titleId = `runtime-confirm-dialog-title-${++dialogCount}`;
  private readonly bodyId = `runtime-confirm-dialog-body-${dialogCount}`;

  createRenderRoot() {
    return this;
  }

  connectedCallback(): void {
    super.connectedCallback();
    document.addEventListener('keydown', this.onDocumentKeydown, true);
    document.addEventListener('focusin', this.onDocumentFocusIn, true);
  }

  disconnectedCallback(): void {
    document.removeEventListener('keydown', this.onDocumentKeydown, true);
    document.removeEventListener('focusin', this.onDocumentFocusIn, true);
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

  private readonly onDocumentKeydown = (event: KeyboardEvent): void => {
    if (!this.isConnected) return;
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
  };

  private readonly onDocumentFocusIn = (event: FocusEvent): void => {
    if (!this.isConnected) return;
    const target = event.target;
    if (target instanceof Node && this.contains(target)) return;
    this.primaryButton()?.focus();
  };

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
