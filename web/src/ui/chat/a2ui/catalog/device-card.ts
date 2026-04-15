import { html, nothing, type TemplateResult } from "lit";
import { renderSparkline } from "./sparkline.js";

const STATUS_CONFIG: Record<string, { color: string; glow: string; label: string; pulse: boolean }> = {
  online:  { color: "#00d4aa", glow: "rgba(0, 212, 170, 0.4)",  label: "在线",   pulse: true  },
  offline: { color: "#6b7280", glow: "rgba(107, 114, 128, 0.3)", label: "离线",   pulse: false },
  warning: { color: "#f59e0b", glow: "rgba(245, 158, 11, 0.4)", label: "告警",   pulse: true  },
  error:   { color: "#ef4444", glow: "rgba(239, 68, 68, 0.4)",  label: "故障",   pulse: true  },
};

export function renderDeviceCard(
  data: Record<string, unknown>,
  onAction?: (fn: string, args: Record<string, unknown>) => void,
): TemplateResult {
  const deviceId = String(data.deviceId || "");
  const deviceName = String(data.name || data.deviceName || deviceId);
  const status = String(data.status || "offline");
  const properties = (data.properties as Array<{ key: string; value: string; unit?: string }>) || [];
  const telemetry = (data.telemetry as Array<{ key: string; value: string; unit?: string }>) || [];
  const sparkline = data.sparkline as number[] | undefined;
  const actions = (data.actions as Array<{ label: string; functionId: string }>) || [];

  const cfg = STATUS_CONFIG[status] || STATUS_CONFIG.offline;

  return html`
    <div class="a2ui-device-card a2ui-device-card--${status}" style="--status-color: ${cfg.color}; --status-glow: ${cfg.glow};">
      <!-- Header -->
      <div class="a2ui-device-card__header">
        <div class="a2ui-device-card__identity">
          <span class="a2ui-device-card__status ${cfg.pulse ? 'a2ui-device-card__status--pulse' : ''}"></span>
          <span class="a2ui-device-card__name" title="${deviceName}">${deviceName}</span>
        </div>
        <span class="a2ui-device-card__badge">${cfg.label}</span>
      </div>

      <!-- Properties List -->
      ${properties.length ? html`
        <div class="a2ui-device-card__props">
          ${properties.map((p) => html`
            <div class="a2ui-device-card__prop">
              <span class="a2ui-device-card__prop-key">${p.key}</span>
              <span class="a2ui-device-card__prop-val">
                ${p.value}${p.unit ? html`<span class="a2ui-device-card__prop-unit">${p.unit}</span>` : nothing}
              </span>
            </div>
          `)}
        </div>
      ` : nothing}

      <!-- Telemetry Grid -->
      ${telemetry.length ? html`
        <div class="a2ui-device-card__telemetry">
          ${telemetry.slice(0, 4).map((t) => html`
            <div class="a2ui-device-card__metric">
              <span class="a2ui-device-card__metric-key">${t.key}</span>
              <span class="a2ui-device-card__metric-val">
                ${t.value}${t.unit ? html`<span class="a2ui-device-card__metric-unit">${t.unit}</span>` : nothing}
              </span>
            </div>
          `)}
        </div>
      ` : nothing}

      <!-- Sparkline Trend -->
      ${sparkline?.length ? html`
        <div class="a2ui-device-card__sparkline">
          ${renderSparkline(sparkline, 140, 32, cfg.color)}
        </div>
      ` : nothing}

      <!-- Footer Actions -->
      ${actions.length ? html`
        <div class="a2ui-device-card__actions">
          ${actions.slice(0, 3).map((a) => html`
            <button class="a2ui-btn a2ui-btn--outline a2ui-btn--xs"
                    @click=${() => { if (onAction) onAction(a.functionId, { deviceId }); }}>
              ${a.label}
            </button>
          `)}
        </div>
      ` : nothing}

      <!-- Device ID Footer -->
      <div class="a2ui-device-card__footer">
        <span class="a2ui-device-card__id">${deviceId}</span>
      </div>
    </div>
  `;
}
