import { LitElement, html, css } from "lit";
import { customElement, state } from "lit/decorators.js";
import { pairGateway } from "../../api/gateway";

@customElement("gateway-pairing-dialog")
export class GatewayPairingDialog extends LitElement {
  static styles = css`
    :host {
      display: block;
    }
    .dialog-overlay {
      position: fixed;
      inset: 0;
      background: rgba(0, 0, 0, 0.5);
      display: flex;
      align-items: center;
      justify-content: center;
      z-index: 1000;
    }
    .dialog {
      background: white;
      border-radius: 12px;
      padding: 32px;
      width: 400px;
      max-width: 90vw;
    }
    .dialog h2 {
      margin: 0 0 8px;
      font-size: 20px;
    }
    .dialog p {
      color: #666;
      margin: 0 0 24px;
    }
    .code-input {
      display: flex;
      gap: 8px;
      justify-content: center;
      margin-bottom: 24px;
    }
    .code-input input {
      width: 100%;
      height: 56px;
      text-align: center;
      font-size: 24px;
      border: 2px solid #ddd;
      border-radius: 8px;
      letter-spacing: 8px;
    }
    .code-input input:focus {
      border-color: #4f46e5;
      outline: none;
    }
    .actions {
      display: flex;
      gap: 12px;
      justify-content: flex-end;
    }
    .btn {
      padding: 8px 20px;
      border-radius: 8px;
      border: none;
      cursor: pointer;
      font-size: 14px;
    }
    .btn-primary {
      background: #4f46e5;
      color: white;
    }
    .btn-primary:disabled {
      opacity: 0.5;
      cursor: not-allowed;
    }
    .btn-cancel {
      background: #f3f4f6;
      color: #374151;
    }
    .error {
      color: #dc2626;
      font-size: 13px;
      margin-bottom: 16px;
    }
    .success {
      color: #16a34a;
      font-size: 14px;
      margin-bottom: 16px;
    }
  `;

  @state() private code = "";
  @state() private loading = false;
  @state() private error = "";
  @state() private success = false;

  render() {
    return html`
      <div class="dialog-overlay" @click=${this.handleOverlayClick}>
        <div class="dialog" @click=${(e: Event) => e.stopPropagation()}>
          <h2>Add Gateway Device</h2>
          <p>Enter the 6-digit code shown on your gateway screen.</p>
          ${this.error ? html`<div class="error">${this.error}</div>` : ""}
          ${this.success
            ? html`<div class="success">Gateway paired successfully!</div>`
            : ""}
          <div class="code-input">
            <input
              type="text"
              maxlength="6"
              .value=${this.code}
              @input=${this.handleCodeInput}
              @keydown=${this.handleKeyDown}
              placeholder="000000"
              ?disabled=${this.loading || this.success}
            />
          </div>
          <div class="actions">
            <button
              class="btn btn-cancel"
              @click=${this.close}
              ?disabled=${this.loading}
            >
              Cancel
            </button>
            <button
              class="btn btn-primary"
              @click=${this.pair}
              ?disabled=${this.loading || this.success || this.code.length !== 6}
            >
              ${this.loading ? "Pairing..." : "Pair"}
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
    if (e.key === "Enter" && this.code.length === 6 && !this.loading) {
      this.pair();
    }
  }

  private async pair() {
    if (this.code.length !== 6) return;
    this.loading = true;
    this.error = "";
    try {
      await pairGateway({ code: this.code });
      this.success = true;
      setTimeout(() => {
        this.close();
        window.location.reload();
      }, 1500);
    } catch (e: any) {
      this.error = e.message || "Pairing failed";
    } finally {
      this.loading = false;
    }
  }

  private handleOverlayClick() {
    if (!this.loading) this.close();
  }

  private close() {
    this.dispatchEvent(new CustomEvent("close"));
  }
}
