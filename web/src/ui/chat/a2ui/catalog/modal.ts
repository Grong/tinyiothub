import { html, nothing, type TemplateResult } from "lit";

export function renderA2uiModal(data: Record<string, unknown>, onAction?: (fn: string, args: Record<string, unknown>) => void): TemplateResult {
  const title = String(data.title || "");
  const content = String(data.content || "");
  const open = Boolean(data.open);
  const actions = (data.actions as Array<{ label: string; functionId: string; variant?: string }>) || [];

  if (!open) return html``;

  return html`
    <div class="a2ui-modal-overlay" @click=${(e: Event) => {
      if (e.target === e.currentTarget && onAction) onAction("close", {});
    }}>
      <div class="a2ui-modal">
        <div class="a2ui-modal__header">
          <h3 class="a2ui-modal__title">${title}</h3>
          <button class="a2ui-modal__close" @click=${() => { if (onAction) onAction("close", {}); }}>✕</button>
        </div>
        <div class="a2ui-modal__body">${content}</div>
        ${actions.length ? html`
          <div class="a2ui-modal__actions">
            ${actions.map((a) => html`
              <button
                class="a2ui-btn a2ui-btn--${a.variant || "secondary"}"
                @click=${() => { if (onAction) onAction(a.functionId, {}); }}
              >${a.label}</button>
            `)}
          </div>
        ` : nothing}
      </div>
    </div>
  `;
}
