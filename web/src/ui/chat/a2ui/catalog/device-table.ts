import { html, nothing, type TemplateResult } from "lit";
import { safeStr, normalizeColumns } from "./utils.js";

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
  const title = safeStr(data.title, "");
  const columns = normalizeColumns(data.columns, ["设备名称", "状态", "最新数据", "操作"]);
  const rawDevices = data.devices as Array<Record<string, unknown>> | undefined;
  const rawRows = data.rows as Array<unknown> | undefined;
  const devices: Array<Record<string, unknown>> = rawDevices
    ? rawDevices
    : rawRows
      ? rawRows.map((r) => {
          const arr = Array.isArray(r) ? r : [];
          return {
            name: arr[0],
            deviceType: arr[1],
            status: arr[2],
            address: arr[3],
            id: arr[4],
          };
        })
      : [];

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
                <td>${safeStr(d.name || d.id, "")}</td>
                <td>
                  <span class="a2ui-device-table__status" style="color: ${statusColor}">
                    ● ${status}
                  </span>
                </td>
                <td>${safeStr(d.latestData, safeStr(d.id, ""))}</td>
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
