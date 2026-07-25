/**
 * A2UI Fallback Component — renders when A2UI component rendering fails.
 *
 * Shows a collapsed <details> block with the raw JSON data. This is the
 * D7 "渲染失败降级为 JSON 原文折叠块" requirement.
 *
 * Usage:
 *   <a2ui-fallback .data=${jsonData} .message=${errorMessage}></a2ui-fallback>
 */
import { LitElement, html, css, nothing } from "lit";
import { customElement, property } from "lit/decorators.js";

@customElement("a2ui-fallback")
export class A2uiFallback extends LitElement {
  static override styles = css`
    :host {
      display: block;
      margin: 8px 0;
    }

    details {
      border: 1px solid var(--border, #e5e7eb);
      border-radius: 8px;
      background: var(--card-bg, #fff);
      overflow: hidden;
      font-family: ui-monospace, SFMono-Regular, Menlo, Monaco, Consolas,
        monospace;
    }

    summary {
      padding: 10px 14px;
      cursor: pointer;
      font-size: 13px;
      color: var(--muted, #6b7280);
      user-select: none;
      display: flex;
      align-items: center;
      gap: 8px;
    }

    summary:hover {
      background: var(--hover-bg, #f9fafb);
    }

    .a2ui-fallback__icon {
      font-size: 14px;
      line-height: 1;
    }

    .a2ui-fallback__message {
      color: var(--warning, #f59e0b);
      font-size: 12px;
      padding: 6px 14px;
      background: rgba(245, 158, 11, 0.08);
      border-top: 1px solid var(--border, #e5e7eb);
    }

    pre {
      margin: 0;
      padding: 12px 14px;
      font-size: 12px;
      line-height: 1.5;
      color: var(--text, #374151);
      overflow-x: auto;
      white-space: pre-wrap;
      word-break: break-word;
      max-height: 400px;
      overflow-y: auto;
      border-top: 1px solid var(--border, #e5e7eb);
    }
  `;

  @property({ type: Object })
  data: Record<string, unknown> | null = null;

  @property({ type: String })
  message = "";

  override render() {
    const json = this.data
      ? JSON.stringify(this.data, null, 2)
      : "{}";

    return html`
      <details>
        <summary>
          <span class="a2ui-fallback__icon">&#9881;</span>
          A2UI 组件数据 (JSON)
          ${this.message
            ? html`<span style="color: var(--warning, #f59e0b); font-size: 11px;">
                  — ${this.message}
                </span>`
            : nothing}
        </summary>
        ${this.message
          ? html`<div class="a2ui-fallback__message">
                ${this.message}
              </div>`
          : nothing}
        <pre>${json}</pre>
      </details>
    `;
  }
}

declare global {
  interface HTMLElementTagNameMap {
    "a2ui-fallback": A2uiFallback;
  }
}
