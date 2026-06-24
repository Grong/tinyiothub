import { html, nothing, type TemplateResult } from "lit";
import { safeStr } from "./utils.js";

export function renderA2uiCard(data: Record<string, unknown>): TemplateResult {
  const title = data.title as string | undefined;
  return html`
    <div class="a2ui-device-card">
      ${title ? html`<div class="a2ui-device-card__name">${title}</div>` : nothing}
      <div class="a2ui-card-body">${safeStr(data.content, "")}</div>
    </div>
  `;
}
