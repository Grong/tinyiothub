import { html, nothing, type TemplateResult } from "lit";

const LEVEL_COLORS: Record<string, string> = {
  info: "#3498db",
  warning: "#f39c12",
  error: "#e74c3c",
  critical: "#9b59b6",
};

const LEVEL_LABELS: Record<string, string> = {
  info: "提示",
  warning: "警告",
  error: "错误",
  critical: "严重",
};

const STATUS_COLORS: Record<string, string> = {
  active: "#e74c3c",
  acknowledged: "#f39c12",
  resolved: "#2ecc71",
};

export function renderAlarmCard(
  data: Record<string, unknown>,
  _onAction?: (fn: string, args: Record<string, unknown>) => void,
): TemplateResult {
  const alarmId = String(data.id || "");
  const deviceName = String(data.deviceName || data.device_id || "未知设备");
  const level = String(data.level || "info").toLowerCase();
  const message = String(data.message || "");
  const status = String(data.status || "active");
  const createdAt = data.created_at as string | undefined;
  const acknowledgedAt = data.acknowledged_at as string | undefined;
  const resolvedAt = data.resolved_at as string | undefined;

  const levelColor = LEVEL_COLORS[level] || "#95a5a6";
  const levelLabel = LEVEL_LABELS[level] || level;
  const statusColor = STATUS_COLORS[status] || "#95a5a6";

  const timeStr = createdAt
    ? new Date(createdAt).toLocaleString([], { month: "numeric", day: "numeric", hour: "numeric", minute: "2-digit" })
    : "";

  const badgeStyle = `background: ${levelColor}; color: white; padding: 1px 6px; border-radius: 4px; font-size: 11px;`;

  return html`
    <div class="a2ui-alarm-card">
      <div class="a2ui-alarm-card__header">
        <span class="a2ui-alarm-card__badge" style=${badgeStyle}>${levelLabel}</span>
        <span class="a2ui-alarm-card__status" style="color: ${statusColor}">● ${status}</span>
      </div>
      <div class="a2ui-alarm-card__message">${message}</div>
      <div class="a2ui-alarm-card__meta">
        <span>${deviceName}</span>
        <span>${timeStr}</span>
      </div>
      ${status === "active" && _onAction ? html`
        <div class="a2ui-alarm-card__actions">
          <button class="a2ui-btn a2ui-btn--secondary a2ui-btn--sm"
                  @click=${() => { _onAction?.("acknowledgeAlarm", { alarmId }); }}>
            确认
          </button>
        </div>
      ` : nothing}
    </div>
  `;
}
