import { html, type TemplateResult } from "lit";

export function renderControlPanel(data: Record<string, unknown>): TemplateResult {
  const deviceId = String(data.deviceId || "");
  return html`
    <div class="a2ui-device-card">
      <div class="a2ui-device-card__name">控制面板: ${deviceId}</div>
      <div class="a2ui-caption">控制面板组件（即将支持）</div>
    </div>
  `;
}
