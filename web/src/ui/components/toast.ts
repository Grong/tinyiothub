import { LitElement, html, css, nothing } from "lit";
import { customElement, state } from "lit/decorators.js";

export interface Toast {
  id: number;
  type: "success" | "error" | "warn" | "info";
  message: string;
  duration?: number;
  action?: { label: string; onClick: () => void };
}

@customElement("toast-container")
export class ToastContainer extends LitElement {
  static styles = css`
    :host {
      position: fixed;
      bottom: 24px;
      right: 24px;
      z-index: 9999;
      display: flex;
      flex-direction: column;
      gap: 8px;
      pointer-events: none;
    }

    .toast {
      padding: 14px 20px;
      border-radius: 10px;
      background: var(--card, #1a1d25);
      border: 1px solid var(--border, #27272a);
      box-shadow: var(--shadow-lg, 0 8px 24px rgba(0, 0, 0, 0.4));
      animation: slideIn 0.3s ease-out;
      display: flex;
      align-items: center;
      gap: 12px;
      max-width: 380px;
      pointer-events: auto;
      font-size: 14px;
      line-height: 1.5;
      color: var(--text);
    }

    .toast.success { border-left: 4px solid var(--ok, #22c55e); }
    .toast.error   { border-left: 4px solid var(--danger, #ef4444); }
    .toast.warn    { border-left: 4px solid var(--warn, #f59e0b); }
    .toast.info    { border-left: 4px solid var(--info, #3b82f6); }

    .toast-icon {
      font-size: 18px;
      flex-shrink: 0;
    }

    .toast.success .toast-icon { color: var(--ok, #22c55e); }
    .toast.error   .toast-icon { color: var(--danger, #ef4444); }
    .toast.warn    .toast-icon { color: var(--warn, #f59e0b); }
    .toast.info    .toast-icon { color: var(--info, #3b82f6); }

    .toast-action {
      background: none;
      border: 1px solid var(--border);
      color: var(--text);
      cursor: pointer;
      padding: 4px 12px;
      border-radius: 6px;
      font-size: 13px;
      font-weight: 600;
      white-space: nowrap;
      transition: background 0.15s;
    }

    .toast-action:hover {
      background: var(--bg-hover);
    }

    .toast-close {
      background: none;
      border: none;
      color: var(--muted);
      cursor: pointer;
      padding: 4px;
      margin-left: auto;
      font-size: 18px;
      line-height: 1;
      transition: color 0.15s;
    }

    .toast-close:hover {
      color: var(--text);
    }

    @keyframes slideIn {
      from {
        opacity: 0;
        transform: translateX(100%);
      }
      to {
        opacity: 1;
        transform: translateX(0);
      }
    }

    @keyframes slideOut {
      from {
        opacity: 1;
        transform: translateX(0);
      }
      to {
        opacity: 0;
        transform: translateX(100%);
      }
    }

    .toast.removing {
      animation: slideOut 0.3s ease-out forwards;
    }

    @media (max-width: 768px) {
      :host {
        left: 16px;
        right: 16px;
        bottom: 16px;
      }
      .toast {
        max-width: 100%;
      }
    }
  `;

  @state()
  toasts: Toast[] = [];

  private idCounter = 0;
  private icons: Record<string, string> = {
    success: "✓",
    error: "✕",
    warn: "⚠",
    info: "ℹ",
  };

  show(type: Toast["type"], message: string, duration = 4000, action?: { label: string; onClick: () => void }) {
    const id = ++this.idCounter;
    const toast: Toast = { id, type, message, duration, action };

    this.toasts = [...this.toasts, toast];

    if (duration > 0) {
      setTimeout(() => this.removeToast(id), duration);
    }
  }

  removeToast(id: number) {
    const toast = this.toasts.find(t => t.id === id);
    if (toast) {
      // 添加 removing class 进行动画
      const el = this.shadowRoot?.querySelector(`[data-id="${id}"]`);
      if (el) {
        el.classList.add("removing");
        setTimeout(() => {
          this.toasts = this.toasts.filter(t => t.id !== id);
        }, 300);
      } else {
        this.toasts = this.toasts.filter(t => t.id !== id);
      }
    }
  }

  // 静态方法，便于从外部调用
  static show(type: Toast["type"], message: string, duration?: number, action?: { label: string; onClick: () => void }) {
    const container = document.querySelector("toast-container") as any;
    if (container) {
      container.show(type, message, duration, action);
    }
  }

  render() {
    return html`
      ${this.toasts.map(toast => html`
        <div
          class="toast ${toast.type}"
          data-id="${toast.id}"
          @click=${() => this.removeToast(toast.id)}
        >
          <span class="toast-icon">${this.icons[toast.type]}</span>
          <span>${toast.message}</span>
          ${toast.action ? html`
            <button class="toast-action" @click=${(e: Event) => {
              e.stopPropagation();
              toast.action!.onClick();
              this.removeToast(toast.id);
            }}>${toast.action.label}</button>
          ` : nothing}
          <button class="toast-close" @click=${(e: Event) => {
            e.stopPropagation();
            this.removeToast(toast.id);
          }}>×</button>
        </div>
      `)}
    `;
  }
}

// 全局 toast 函数
export function showToast(type: Toast["type"], message: string, duration?: number, action?: { label: string; onClick: () => void }) {
  const container = document.querySelector("toast-container") as any;
  if (container?.show) {
    container.show(type, message, duration, action);
  } else {
    // 如果没有容器，直接用 alert
    console.log(`[${type}] ${message}`);
  }
}

export function success(message: string, action?: { label: string; onClick: () => void }) { showToast("success", message, action ? 8000 : undefined, action); }
export function error(message: string) { showToast("error", message); }
export function warn(message: string) { showToast("warn", message); }
export function info(message: string) { showToast("info", message); }
