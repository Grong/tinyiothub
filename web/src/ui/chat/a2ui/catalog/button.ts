import { html, type TemplateResult } from "lit";

export function renderA2uiButton(data: Record<string, unknown>, onAction?: (fn: string, args: Record<string, unknown>) => void): TemplateResult {
  const text = String(data.text || "Button");
  const variant = String(data.variant || "primary");
  const disabled = Boolean(data.disabled);
  const functionId = data.functionId as string | undefined;

  return html`
    <button class="a2ui-btn a2ui-btn--${variant}"
      ?disabled=${disabled}
      @click=${() => { if (functionId && onAction) onAction(functionId, {}); }}>
      ${text}
    </button>
  `;
}
