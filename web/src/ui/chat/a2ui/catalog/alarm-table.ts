import { html, nothing, type TemplateResult } from "lit";

const LEVEL_COLORS: Record<string, string> = {
  info: "#3498db",
  warning: "#f39c12",
  error: "#e74c3c",
  critical: "#9b59b6",
};

const STATUS_COLORS: Record<string, string> = {
  active: "#e74c3c",
  acknowledged: "#f39c12",
  resolved: "#2ecc71",
};

export function renderAlarmTable(
  data: Record<string, unknown>,
  _onAction?: (fn: string, args: Record<string, unknown>) => void,
): TemplateResult {
  const title = String(data.title || "");
  const columns = (data.columns as string[]) || ["设备", "级别", "消息", "状态", "时间"];
  const alarms = (data.alarms as Array<Record<string, unknown>>) || [];

  return html`
    <div class="a2ui-alarm-table">
      ${title ? html`<div class="a2ui-alarm-table__title">${title}</div>` : nothing}
      <table class="a2ui-alarm-table__table">
        <thead>
          <tr>${columns.map((c) => html`<th>${c}</th>`)}</tr>
        </thead>
        <tbody>
          ${alarms.map((a) => {
            const level = String(a.level || "info").toLowerCase();
            const status = String(a.status || "active");
            const levelColor = LEVEL_COLORS[level] || "#95a5a6";
            const statusColor = STATUS_COLORS[status] || "#95a5a6";
            const timeStr = a.created_at
              ? new Date(a.created_at as string).toLocaleString([], { month: "numeric", day: "numeric", hour: "numeric", minute: "2-digit" })
              : "";
            return html`
              <tr>
                <td>${String(a.deviceName || a.device_id || "—")}</td>
                <td><span style="color: ${levelColor}; font-weight: 500;">${level}</span></td>
                <td>${String(a.message || "—")}</td>
                <td><span style="color: ${statusColor};">● ${status}</span></td>
                <td>${timeStr}</td>
              </tr>
            `;
          })}
        </tbody>
      </table>
      ${alarms.length === 0 ? html`<div class="a2ui-caption" style="padding: 12px">暂无告警</div>` : nothing}
    </div>
  `;
}
