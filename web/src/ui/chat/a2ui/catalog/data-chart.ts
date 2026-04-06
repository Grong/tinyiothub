import { html, type TemplateResult } from "lit";

export function renderDataChart(data: Record<string, unknown>): TemplateResult {
  const title = String(data.title || "图表");
  return html`
    <div class="a2ui-device-card">
      <div class="a2ui-device-card__name">${title}</div>
      <div class="a2ui-caption">图表组件（即将支持）</div>
    </div>
  `;
}
