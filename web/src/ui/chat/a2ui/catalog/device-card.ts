import { html, nothing, type TemplateResult } from "lit";
import { renderSparkline } from "./sparkline.js";

const STATUS_COLORS: Record<string, string> = {
  online: "#2ecc71",
  offline: "#95a5a6",
  warning: "#f39c12",
  error: "#e74c3c",
};

const STATUS_LABELS: Record<string, string> = {
  online: "在线",
  offline: "离线",
  warning: "告警",
  error: "故障",
};

export function renderDeviceCard(
  data: Record<string, unknown>,
  onAction?: (fn: string, args: Record<string, unknown>) => void,
): TemplateResult {
  const deviceId = String(data.deviceId || "");
  const deviceName = String(data.name || data.deviceName || deviceId);
  const status = String(data.status || "unknown");
  const telemetry = (data.telemetry as Array<{ key: string; value: string; unit: string }>) || [];
  const sparkline = data.sparkline as number[] | undefined;
  const lastSeen = data.lastSeen as string | undefined;
  const actions = (data.actions as Array<{ label: string; functionId: string }>) || [];

  const statusColor = STATUS_COLORS[status] || "#95a5a6";
  const statusLabel = STATUS_LABELS[status] || status;

  return html`
    <div class="a2ui-device-card">
      <div class="a2ui-device-card__header">
        <span class="a2ui-device-card__status" style="background: ${statusColor}"></span>
        <span class="a2ui-device-card__name">${deviceName}</span>
        <span class="a2ui-device-card__badge" style="color: ${statusColor}">${statusLabel}</span>
      </div>

      ${telemetry.length ? html`
        <div class="a2ui-device-card__telemetry">
          ${telemetry.map((t) => html`
            <div class="a2ui-device-card__metric">
              <span class="a2ui-device-card__metric-key">${t.key}</span>
              <span class="a2ui-device-card__metric-value">${t.value}${t.unit ? ` ${t.unit}` : ""}</span>
            </div>
          `)}
        </div>
      ` : nothing}

      ${sparkline?.length ? html`
        <div class="a2ui-device-card__sparkline">
          ${renderSparkline(sparkline, 120, 28, statusColor)}
        </div>
      ` : nothing}

      ${lastSeen ? html`
        <div class="a2ui-device-card__last-seen">
          最后活跃: ${new Date(lastSeen).toLocaleTimeString([], { hour: "numeric", minute: "2-digit" })}
        </div>
      ` : nothing}

      ${actions.length ? html`
        <div class="a2ui-device-card__actions">
          ${actions.map((a) => html`
            <button class="a2ui-btn a2ui-btn--secondary a2ui-btn--sm"
                    @click=${() => { if (onAction) onAction(a.functionId, { deviceId }); }}>
              ${a.label}
            </button>
          `)}
        </div>
      ` : nothing}

      <div class="a2ui-device-card__id">${deviceId}</div>
    </div>
  `;
}
