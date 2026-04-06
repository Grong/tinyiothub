import { html, nothing, type TemplateResult } from "lit";

export function renderDeviceTable(data: Record<string, unknown>): TemplateResult {
  const devices = (data.devices as Array<Record<string, unknown>>) || [];
  return html`
    <div class="a2ui-device-table">
      <table>
        <thead><tr><th>设备</th><th>状态</th><th>ID</th></tr></thead>
        <tbody>
          ${devices.map(d => html`
            <tr>
              <td>${String(d.name || d.id || "")}</td>
              <td>${String(d.status || "unknown")}</td>
              <td>${String(d.id || "")}</td>
            </tr>
          `)}
        </tbody>
      </table>
      ${devices.length === 0 ? html`<div class="a2ui-caption">暂无设备</div>` : nothing}
    </div>
  `;
}
