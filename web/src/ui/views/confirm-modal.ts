import { LitElement, html } from "lit";
import { customElement, property, state } from "lit/decorators.js";

/**
 * Confirm Modal (D13) — reusable action confirmation dialog.
 *
 * Usage:
 *   <confirm-modal
 *     .open=${boolean}
 *     .actionName=${string}
 *     .thingName=${string}
 *     .parameters=${Record<string, string>}
 *     .danger=${boolean}
 *     .loading=${boolean}
 *     @confirm=${() => void}
 *     @cancel=${() => void}
 *   ></confirm-modal>
 */
@customElement("confirm-modal")
export class ConfirmModal extends LitElement {
  @property({ type: Boolean }) open = false;
  @property({ type: String }) actionName = "";
  @property({ type: String }) thingName = "";
  @property({ type: Object }) parameters: Record<string, string> = {};
  @property({ type: Boolean }) danger = false;
  @property({ type: Boolean }) loading = false;

  @state() private _lastFocus: Element | null = null;

  createRenderRoot() {
    return this;
  }

  connectedCallback() {
    super.connectedCallback();
  }

  updated(changedProperties: Map<string, unknown>) {
    if (changedProperties.has("open")) {
      if (this.open) {
        this._lastFocus = document.activeElement;
        // Focus-trap: focus first focusable element
        requestAnimationFrame(() => {
          const first = this.querySelector<HTMLElement>(
            'button, [href], input, select, textarea, [tabindex]:not([tabindex="-1"])'
          );
          first?.focus();
        });
      } else if (this._lastFocus) {
        const el = this._lastFocus as HTMLElement;
        if (el?.focus) {
          requestAnimationFrame(() => el.focus());
        }
        this._lastFocus = null;
      }
    }
  }

  private _handleKeydown(e: KeyboardEvent) {
    if (e.key === "Escape") {
      e.preventDefault();
      this._cancel();
      return;
    }
    if (e.key === "Tab") {
      const focusables = Array.from(
        this.querySelectorAll<HTMLElement>(
          'a[href], button, textarea, input:not([type="hidden"]), select, [tabindex]:not([tabindex="-1"])'
        )
      ).filter(
        (el) => !el.hasAttribute("disabled") && (el as HTMLElement).offsetParent !== null
      );
      if (focusables.length === 0) return;
      const first = focusables[0];
      const last = focusables[focusables.length - 1];
      if (e.shiftKey) {
        if (document.activeElement === first) {
          e.preventDefault();
          last.focus();
        }
      } else {
        if (document.activeElement === last) {
          e.preventDefault();
          first.focus();
        }
      }
    }
  }

  private _confirm() {
    if (this.loading) return;
    this.dispatchEvent(new CustomEvent("confirm", { bubbles: true, composed: true }));
  }

  private _cancel() {
    if (this.loading) return;
    this.dispatchEvent(new CustomEvent("cancel", { bubbles: true, composed: true }));
  }

  render() {
    if (!this.open) return html``;

    const paramEntries = Object.entries(this.parameters);
    const hasParams = paramEntries.length > 0;

    return html`
      <div
        class="modal-overlay confirm-modal-overlay"
        role="dialog"
        aria-modal="true"
        aria-label="确认操作: ${this.actionName}"
        @keydown=${this._handleKeydown}
      >
        <div class="modal confirm-modal" @click=${(e: Event) => e.stopPropagation()}>
          <div class="modal-header">
            <span>确认操作</span>
            <button
              class="btn btn--icon"
              aria-label="关闭"
              ?disabled=${this.loading}
              @click=${this._cancel}
            >&times;</button>
          </div>

          <div class="modal-body confirm-modal__body">
            <div class="confirm-modal__info">
              <div class="confirm-modal__label">操作</div>
              <div class="confirm-modal__value confirm-modal__action-name">${this.actionName}</div>
            </div>
            ${this.thingName ? html`
              <div class="confirm-modal__info">
                <div class="confirm-modal__label">目标物</div>
                <div class="confirm-modal__value">${this.thingName}</div>
              </div>
            ` : ""}

            ${hasParams ? html`
              <div class="confirm-modal__params">
                <div class="confirm-modal__label" style="margin-bottom: 8px;">参数</div>
                <table class="data-table data-table--compact confirm-modal__param-table">
                  <thead>
                    <tr>
                      <th>参数名</th>
                      <th>值</th>
                    </tr>
                  </thead>
                  <tbody>
                    ${paramEntries.map(([key, value]) => html`
                      <tr>
                        <td>${key}</td>
                        <td><code>${value}</code></td>
                      </tr>
                    `)}
                  </tbody>
                </table>
              </div>
            ` : html`
              <div class="confirm-modal__info">
                <div class="confirm-modal__label">参数</div>
                <div class="confirm-modal__value" style="color: var(--muted); font-style: italic;">无参数</div>
              </div>
            `}
          </div>

          <div class="modal-footer confirm-modal__footer">
            <div class="confirm-modal__hint">
              可在<a href="/settings" class="confirm-modal__link" @click=${(e: Event) => { e.preventDefault(); window.history.pushState({}, "", "/settings"); window.dispatchEvent(new PopStateEvent("popstate")); this._cancel(); }}>工作区设置</a>中关闭动作确认
            </div>
            <div class="confirm-modal__actions">
              <button
                class="btn btn--ghost"
                ?disabled=${this.loading}
                @click=${this._cancel}
              >取消</button>
              <button
                class="btn ${this.danger ? "btn--danger" : "btn--primary"}"
                ?disabled=${this.loading}
                @click=${this._confirm}
              >
                ${this.loading ? "执行中..." : "确认执行"}
              </button>
            </div>
          </div>
        </div>
      </div>
    `;
  }
}
