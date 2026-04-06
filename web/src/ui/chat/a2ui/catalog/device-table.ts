import { html, nothing, type TemplateResult } from "lit";

const STATUS_COLORS: Record<string, string> = {
  online: "#2ecc71",
  offline: "#95a5a6",
  warning: "#f39c12",
  error: "#e74c3c",
};

export function renderDeviceTable(
  data: Record<string, unknown>,
  onAction?: (fn: string, args: Record<string, unknown>) => void,
): TemplateResult {
  const title = String(data.title || "");
  const columns = (data.columns as string[]) || ["设备名称", "状态", "最新数据", "操作"];
  const devices = (data.devices as Array<Record<string, unknown>>) || [];

  return html`
    <div class="a2ui-device-table">
      ${title ? html`<div class="a2ui-device-table__title">${title}</div>` : nothing}
      <table class="a2ui-device-table__table">
        <thead>
          <tr>${columns.map((c) => html`<th>${c}</th>`)}</tr>
        </thead>
        <tbody>
          ${devices.map((d) => {
            const status = String(d.status || "unknown");
            const statusColor = STATUS_COLORS[status] || "#95a5a6";
            const actions = (d.actions as Array<{ label: string; functionId: string }>) || [];

            return html`
              <tr>
                <td>${String(d.name || d.id || "")}</td>
                <td>
                  <span class="a2ui-device-table__status" style="color: ${statusColor}">
                    ● ${status}
                  </span>
                </td>
                <td>${String(d.latestData || d.id || "")}</td>
                <td>
                  <div class="a2ui-device-table__actions">
                    ${actions.map((a) => html`
                      <button class="a2ui-btn a2ui-btn--secondary a2ui-btn--sm"
                              @click=${() => { if (onAction) onAction(a.functionId, { deviceId: d.id }); }}>
                        ${a.label}
                      </button>
                    `)}
                  </div>
                </td>
              </tr>
            `;
          })}
        </tbody>
      </table>
      ${devices.length === 0 ? html`<div class="a2ui-caption" style="padding: 12px">暂无设备</div>` : nothing}
    </div>
  `;
}
