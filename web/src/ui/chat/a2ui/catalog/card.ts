import { html, nothing, type TemplateResult } from "lit";

export function renderA2uiCard(data: Record<string, unknown>): TemplateResult {
  const title = data.title as string | undefined;
  return html`
    <div class="a2ui-device-card">
      ${title ? html`<div class="a2ui-device-card__name">${title}</div>` : nothing}
      <div class="a2ui-card-body">${String(data.content || "")}</div>
    </div>
  `;
}
