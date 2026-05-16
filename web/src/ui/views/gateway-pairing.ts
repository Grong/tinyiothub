import { LitElement, html, css } from "lit";
import { customElement, state } from "lit/decorators.js";
import { pairGateway, type PairingResponse } from "../../api/gateway";
import { i18n, t } from "../../i18n/index.js";

@customElement("gateway-pairing-dialog")
export class GatewayPairingDialog extends LitElement {
  static styles = css`
    :host {
      display: block;
    }

    .modal-overlay {
      position: fixed;
      inset: 0;
      background: var(--overlay-backdrop);
      display: flex;
      align-items: center;
      justify-content: center;
      z-index: var(--z-modal);
      padding: var(--space-4);
      animation: overlay-in 150ms var(--ease-out);
    }

    @keyframes overlay-in {
      from { opacity: 0; }
    }

    .modal-box {
      background: var(--card);
      border-radius: var(--radius-lg);
      border: 1px solid var(--border);
      box-shadow: 0 20px 60px var(--overlay-backdrop);
      width: 100%;
      max-width: 400px;
      animation: modal-in 200ms var(--ease-out);
    }

    @keyframes modal-in {
      from { opacity: 0; transform: translateY(8px) scale(0.98); }
    }

    .modal-header {
      display: flex;
      align-items: center;
      justify-content: space-between;
      padding: 20px 24px 0;
    }

    .modal-header h3 {
      font-size: 16px;
      font-weight: 600;
      color: var(--text);
      margin: 0;
    }

    .modal-close {
      width: 28px;
      height: 28px;
      border-radius: var(--radius-sm);
      border: none;
      background: none;
      color: var(--muted);
      font-size: 18px;
      cursor: pointer;
      display: flex;
      align-items: center;
      justify-content: center;
      transition: color var(--duration-fast) ease;
    }

    .modal-close:hover {
      color: var(--text);
    }

    .modal-body {
      padding: 20px 24px;
    }

    .modal-desc {
      font-size: 14px;
      color: var(--muted);
      margin: 0 0 var(--space-5);
      line-height: 1.5;
    }

    .code-input {
      display: flex;
      gap: 8px;
      justify-content: center;
      margin-bottom: var(--space-5);
    }

    .code-input input {
      width: 100%;
      height: 56px;
      text-align: center;
      font-size: 24px;
      font-family: var(--mono);
      font-weight: 600;
      letter-spacing: 8px;
      background: var(--card);
      border: 2px solid var(--border);
      border-radius: var(--radius-md);
      color: var(--text);
      outline: none;
      transition: border-color var(--duration-fast) ease,
                  box-shadow var(--duration-fast) ease;
    }

    .code-input input:focus {
      border-color: var(--accent);
      box-shadow: 0 0 0 3px var(--accent-subtle);
    }

    .code-input input:disabled {
      opacity: 0.5;
      cursor: not-allowed;
    }

    .form-error {
      font-size: 12px;
      color: var(--danger);
      margin-bottom: var(--space-4);
    }

    .success-box {
      display: flex;
      flex-direction: column;
      gap: var(--space-1);
      padding: var(--space-3);
      margin-bottom: var(--space-4);
      background: var(--ok-subtle);
      border: 1px solid var(--ok-muted);
      border-radius: var(--radius-md);
      font-size: 14px;
      color: var(--ok);
    }

    .success-box .device-name {
      font-weight: 600;
      color: var(--text);
    }

    .success-box .device-meta {
      font-size: 12px;
      color: var(--muted);
    }

    .modal-footer {
      display: flex;
      justify-content: flex-end;
      gap: var(--space-2);
      padding: 0 24px 20px;
    }

    .spinner {
      display: inline-block;
      width: 14px;
      height: 14px;
      border: 2px solid var(--text-inverse);
      border-top-color: transparent;
      border-radius: 50%;
      animation: spin 0.6s linear infinite;
      margin-right: 6px;
      vertical-align: middle;
    }

    @keyframes spin {
      to { transform: rotate(360deg); }
    }

    .btn {
      display: inline-flex;
      align-items: center;
      justify-content: center;
      gap: var(--space-2);
      border: none;
      background: var(--bg-muted);
      padding: 9px var(--space-4);
      border-radius: var(--radius-md);
      font-size: 13px;
      font-weight: 500;
      letter-spacing: -0.01em;
      cursor: pointer;
      transition:
        background var(--duration-fast) var(--ease-out),
        box-shadow var(--duration-fast) var(--ease-out),
        transform var(--duration-fast) var(--ease-out);
      box-shadow: var(--shadow-sm);
      color: var(--text);
    }

    .btn:hover {
      background: var(--bg-hover);
      transform: translateY(-1px);
    }

    .btn:active {
      background: var(--secondary);
      transform: translateY(0);
    }

    .btn.primary {
      background: var(--accent-gradient);
      color: var(--primary-foreground);
      box-shadow: 0 2px 10px var(--accent-glow);
    }

    .btn.primary:hover {
      background: var(--accent-gradient-soft);
      box-shadow: 0 4px 16px var(--accent-glow-strong);
      transform: translateY(-1px);
    }

    .btn:disabled {
      opacity: 0.5;
      cursor: not-allowed;
      transform: none;
      box-shadow: var(--shadow-sm);
    }
  `;

  @state() private code = "";
  @state() private loading = false;
  @state() private error = "";
  @state() private pairedDevice: PairingResponse | null = null;

  private _unsubI18n?: () => void;
  private _onKeyDown?: (e: KeyboardEvent) => void;

  connectedCallback() {
    super.connectedCallback();
    this._unsubI18n = i18n.subscribe(() => this.requestUpdate());
    this._onKeyDown = (e: KeyboardEvent) => {
      if (e.key === "Escape" && !this.loading) {
        this.close();
      }
    };
    document.addEventListener("keydown", this._onKeyDown);
  }

  disconnectedCallback() {
    super.disconnectedCallback();
    this._unsubI18n?.();
    if (this._onKeyDown) {
      document.removeEventListener("keydown", this._onKeyDown);
    }
  }

  render() {
    return html`
      <div
        class="modal-overlay"
        role="dialog"
        aria-modal="true"
        aria-label=${t("gatewayPairing.title")}
        @click=${this.handleOverlayClick}
      >
        <div class="modal-box" @click=${(e: Event) => e.stopPropagation()}>
          <div class="modal-header">
            <h3>${t("gatewayPairing.title")}</h3>
            <button
              class="modal-close"
              @click=${this.close}
              ?disabled=${this.loading}
              aria-label=${t("gatewayPairing.close")}
            >&times;</button>
          </div>

          <div class="modal-body">
            <p class="modal-desc">${t("gatewayPairing.description")}</p>

            ${this.error ? html`<div class="form-error">${this.error}</div>` : ""}
            ${this.pairedDevice
              ? html`
                <div class="success-box">
                  <span class="device-name">${this.pairedDevice.deviceName}</span>
                  <span class="device-meta">
                    ${this.pairedDevice.hostname} &middot; ${this.pairedDevice.ip}
                  </span>
                  <span>${t("gatewayPairing.success")}</span>
                </div>
              `
              : ""}

            <div class="code-input">
              <input
                type="text"
                inputmode="numeric"
                maxlength="6"
                .value=${this.code}
                @input=${this.handleCodeInput}
                @keydown=${this.handleKeyDown}
                placeholder=${t("gatewayPairing.codePlaceholder")}
                ?disabled=${this.loading || !!this.pairedDevice}
                aria-label=${t("gatewayPairing.codeLabel")}
                autocomplete="off"
              />
            </div>
          </div>

          <div class="modal-footer">
            <button
              class="btn"
              @click=${this.close}
              ?disabled=${this.loading}
            >
              ${t("gatewayPairing.cancel")}
            </button>
            <button
              class="btn primary"
              @click=${this.pair}
              ?disabled=${this.loading || !!this.pairedDevice || this.code.length !== 6}
            >
              ${this.loading
                ? html`<span class="spinner"></span>${t("gatewayPairing.pairing")}`
                : this.pairedDevice ? t("gatewayPairing.paired") : t("gatewayPairing.pair")}
            </button>
          </div>
        </div>
      </div>
    `;
  }

  private handleCodeInput(e: InputEvent) {
    const input = e.target as HTMLInputElement;
    this.code = input.value.replace(/\D/g, "").slice(0, 6);
    this.error = "";
  }

  private handleKeyDown(e: KeyboardEvent) {
    if (e.key === "Enter" && this.code.length === 6 && !this.loading && !this.pairedDevice) {
      this.pair();
    }
  }

  private async pair() {
    if (this.code.length !== 6) return;
    this.loading = true;
    this.error = "";
    try {
      const device = await pairGateway({ code: this.code });
      this.pairedDevice = device;
      this.dispatchEvent(
        new CustomEvent("paired", { detail: device, bubbles: true, composed: true })
      );
      setTimeout(() => {
        this.close();
      }, 2000);
    } catch (e: any) {
      this.error = e.message || t("gatewayPairing.pairFailed");
    } finally {
      this.loading = false;
    }
  }

  private handleOverlayClick() {
    if (!this.loading) this.close();
  }

  private close() {
    this.dispatchEvent(new CustomEvent("close", { bubbles: true, composed: true }));
  }
}
