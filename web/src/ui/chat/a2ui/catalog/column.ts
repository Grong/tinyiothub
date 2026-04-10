import { html, type TemplateResult } from "lit";

export function renderA2uiColumn(_data: Record<string, unknown>): TemplateResult {
  return html`<div class="a2ui-column" style="display: flex; flex-direction: column; gap: 8px;"></div>`;
}
