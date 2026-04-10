import { html, type TemplateResult } from "lit";

export function renderA2uiRow(_data: Record<string, unknown>): TemplateResult {
  return html`<div class="a2ui-row" style="display: flex; flex-direction: row; gap: 8px;"></div>`;
}
