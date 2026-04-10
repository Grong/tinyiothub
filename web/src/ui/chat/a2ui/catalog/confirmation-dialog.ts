import { html, type TemplateResult } from "lit";

export function renderConfirmationDialog(data: Record<string, unknown>, onAction?: (fn: string, args: Record<string, unknown>) => void): TemplateResult {
  const title = String(data.title || "确认");
  const message = String(data.message || "");
  const confirmFn = data.confirmFunctionId as string | undefined;
  const cancelFn = data.cancelFunctionId as string | undefined;

  return html`
    <div class="a2ui-device-card" style="border-color: var(--accent);">
      <div class="a2ui-device-card__name">${title}</div>
      <p class="a2ui-text">${message}</p>
      <div style="display: flex; gap: 8px; margin-top: 8px;">
        <button class="a2ui-btn a2ui-btn--primary" @click=${() => { if (confirmFn && onAction) onAction(confirmFn, {}); }}>确认</button>
        <button class="a2ui-btn a2ui-btn--secondary" @click=${() => { if (cancelFn && onAction) onAction(cancelFn, {}); }}>取消</button>
      </div>
    </div>
  `;
}
