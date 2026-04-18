import { html, nothing, type TemplateResult } from "lit";
import { renderSparkline } from "./sparkline.js";

const STATUS_CONFIG: Record<string, { color: string; glow: string; label: string; pulse: boolean }> = {
  online:  { color: "#00d4aa", glow: "rgba(0, 212, 170, 0.4)",  label: "在线",   pulse: true  },
  offline: { color: "#6b7280", glow: "rgba(107, 114, 128, 0.3)", label: "离线",   pulse: false },
  warning: { color: "#f59e0b", glow: "rgba(245, 158, 11, 0.4)", label: "告警",   pulse: true  },
  error:   { color: "#ef4444", glow: "rgba(239, 68, 68, 0.4)",  label: "故障",   pulse: true  },
};

// Normalize property data from various formats
type PropItem = { key: string; value: string; unit?: string };

function normalizeProperties(data: Record<string, unknown>): PropItem[] {
  const arr = data.properties;
  if (!Array.isArray(arr)) {
    // Fallback: flat key-value pairs
    const reserved = new Set(["deviceId", "name", "displayName", "deviceName", "status", "state", "deviceType", "telemetry", "sparkline", "actions", "id", "componentKind", "dataModel"]);
    return Object.entries(data)
      .filter(([k, v]) => !reserved.has(k) && v != null)
      .map(([key, val]) => ({ key, value: String(val) }));
  }
  if (arr.length > 0) {
    const first = arr[0];
    // Format A: { name, displayName, value, unit } — IoT style
    if ("name" in first || "displayName" in first) {
      return arr
        .filter((p: any) => p && (p.displayName || p.name))
        .map((p: any) => ({
          key: String(p.displayName || p.name || ""),
          value: String(p.value ?? ""),
          unit: p.unit ? String(p.unit) : undefined,
        }));
    }
    // Format B: { key, value, unit } — standard style
    return (arr as PropItem[])
      .filter(p => p && typeof p.key === "string")
      .map(p => ({ key: p.key, value: p.value || "", unit: p.unit }));
  }
  return [];
}

function normalizeTelemetry(data: Record<string, unknown>): PropItem[] {
  const arr = data.telemetry;
  if (!Array.isArray(arr)) return [];
  if (arr.length > 0) {
    const first = arr[0];
    // Format A: { name, displayName, value, unit } — IoT style
    if ("name" in first || "displayName" in first) {
      return (arr as any[])
        .filter((t: any) => t && (t.displayName || t.name))
        .map((t: any) => ({
          key: String(t.displayName || t.name || ""),
          value: String(t.value ?? ""),
          unit: t.unit ? String(t.unit) : undefined,
        }));
    }
    // Format B: { key, value, unit } — standard style
    return (arr as PropItem[])
      .filter(t => t && typeof t.key === "string")
      .map(t => ({ key: t.key, value: t.value || "", unit: t.unit }));
  }
  return [];
}

function normalizeStatus(data: Record<string, unknown>): string {
  // Try various status field names
  const statusFields = ["status", "state", "online", "deviceStatus"];
  for (const field of statusFields) {
    if (typeof data[field] === "string" && data[field]) {
      return String(data[field]).toLowerCase();
    }
  }
  // Infer from explicit online flag
  if (data.online === true || data.online === "true") return "online";
  if (data.online === false || data.online === "false") return "offline";
  return "offline";
}

export function renderDeviceCard(
  data: Record<string, unknown>,
  onAction?: (fn: string, args: Record<string, unknown>) => void,
): TemplateResult {
  const deviceId = String(data.deviceId || data.id || "");
  const deviceName = String(data.displayName || data.name || data.deviceName || data.title || deviceId);
  const status = normalizeStatus(data);
  const properties = normalizeProperties(data);
  const telemetry = normalizeTelemetry(data);
  const sparkline = data.sparkline as number[] | undefined;
  const actions = (data.actions as Array<{ label: string; functionId: string }>) || [];

  const cfg = STATUS_CONFIG[status] || STATUS_CONFIG.offline;

  // Debug: log what we received
  console.log("[DeviceCard] Received data:", JSON.stringify(data).substring(0, 300));
  console.log("[DeviceCard] Normalized — name:", deviceName, "status:", status, "properties:", properties.length, "telemetry:", telemetry.length);

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
