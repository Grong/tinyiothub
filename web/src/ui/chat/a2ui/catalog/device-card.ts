import { html, nothing, type TemplateResult } from "lit";
import { renderSparkline } from "./sparkline.js";
import { safeStr } from "./utils.js";

// ── Status config ──
const STATUS_CONFIG: Record<string, { color: string; glow: string; label: string; pulse: boolean }> = {
  online:  { color: "#00d4aa", glow: "rgba(0, 212, 170, 0.4)",  label: "在线",   pulse: true  },
  offline: { color: "#6b7280", glow: "rgba(107, 114, 128, 0.3)", label: "离线",   pulse: false },
  warning: { color: "#f59e0b", glow: "rgba(245, 158, 11, 0.4)", label: "告警",   pulse: true  },
  error:   { color: "#ef4444", glow: "rgba(239, 68, 68, 0.4)",  label: "故障",   pulse: true  },
};

// ── Device type → icon mapping ──
const DEVICE_ICON_MAP: Record<string, string> = {
  temperature: "thermometer",
  humidity: "thermometer",
  sensor: "thermometer",
  "temp_sensor": "thermometer",
  "temp_humidity": "thermometer",
  socket: "wifi",
  switch: "wifi",
  plug: "wifi",
  light: "star",
  lock: "lock",
  camera: "chart",
  gateway: "wifi",
  button: "add",
  alarm: "bell",
  meter: "battery",
};

// ── Signal strength bars ──
function renderSignalBars(strength: number): TemplateResult {
  const level = strength <= 0 ? 0 : strength <= 25 ? 1 : strength <= 50 ? 2 : strength <= 75 ? 3 : 4;
  return html`
    <span class="a2ui-device-card__signal" title="信号: ${strength}%">
      ${[1, 2, 3, 4].map((bar) => html`
        <span class="a2ui-device-card__signal-bar ${bar <= level ? "a2ui-device-card__signal-bar--active" : ""}"></span>
      `)}
    </span>
  `;
}

// ── Relative time formatter ──
function formatRelativeTime(isoOrString: string): string {
  try {
    const date = new Date(isoOrString);
    if (isNaN(date.getTime())) return isoOrString;
    const now = Date.now();
    const diffMs = now - date.getTime();
    const diffSec = Math.floor(diffMs / 1000);
    if (diffSec < 5) return "刚刚";
    if (diffSec < 60) return `${diffSec} 秒前`;
    const diffMin = Math.floor(diffSec / 60);
    if (diffMin < 60) return `${diffMin} 分钟前`;
    const diffHr = Math.floor(diffMin / 60);
    if (diffHr < 24) return `${diffHr} 小时前`;
    const diffDay = Math.floor(diffHr / 24);
    if (diffDay < 30) return `${diffDay} 天前`;
    return date.toLocaleDateString("zh-CN");
  } catch {
    return isoOrString;
  }
}

// ── Normalizers ──
type PropItem = { key: string; value: string; unit?: string };

function normalizeProperties(data: Record<string, unknown>): PropItem[] {
  const arr = data.properties;
  if (!Array.isArray(arr)) {
    const reserved = new Set(["deviceId", "id", "name", "displayName", "deviceName", "title",
      "status", "state", "deviceType", "icon", "primaryMetric", "telemetry", "sparkline",
      "actions", "componentKind", "dataModel", "lastSeen", "updatedAt", "signalStrength", "tags"]);
    return Object.entries(data)
      .filter(([k, v]) => !reserved.has(k) && v != null)
      .map(([key, val]) => ({ key, value: safeStr(val) }));
  }
  if (arr.length === 0) return [];
  const first = arr[0];
  if ("name" in first || "displayName" in first) {
    return (arr as any[])
      .filter((p: any) => p && (p.displayName || p.name))
      .map((p: any) => ({
        key: String(p.displayName || p.name || ""),
        value: safeStr(p.value, ""),
        unit: p.unit ? String(p.unit) : undefined,
      }));
  }
  return (arr as PropItem[])
    .filter(p => p && typeof p.key === "string")
    .map(p => ({ key: p.key, value: p.value || "", unit: p.unit }));
}

function normalizeTelemetry(data: Record<string, unknown>): PropItem[] {
  const arr = data.telemetry;
  if (!Array.isArray(arr) || arr.length === 0) return [];
  const first = arr[0];
  if ("name" in first || "displayName" in first) {
    return (arr as any[])
      .filter((t: any) => t && (t.displayName || t.name))
      .map((t: any) => ({
        key: String(t.displayName || t.name || ""),
        value: String(t.value ?? ""),
        unit: t.unit ? String(t.unit) : undefined,
      }));
  }
  return (arr as PropItem[])
    .filter(t => t && typeof t.key === "string")
    .map(t => ({ key: t.key, value: t.value || "", unit: t.unit }));
}

function normalizePrimaryMetric(data: Record<string, unknown>, properties: PropItem[]): PropItem | null {
  const pm = data.primaryMetric as Record<string, unknown> | undefined;
  if (pm && pm.value != null) {
    return {
      key: String(pm.key || pm.name || ""),
      value: safeStr(pm.value),
      unit: pm.unit ? String(pm.unit) : undefined,
    };
  }
  // Auto-detect: first property that looks like a primary metric (temperature, humidity, etc.)
  const primaryKeys = ["温度", "Temperature", "temp", "temperature", "湿度", "Humidity", "humidity", "电量", "Battery", "battery", "功率", "Power", "power", "电压", "Voltage", "voltage"];
  const match = properties.find(p => primaryKeys.some(k => p.key.toLowerCase().includes(k.toLowerCase())));
  return match || (properties.length > 0 ? properties[0] : null);
}

function normalizeStatus(data: Record<string, unknown>): string {
  const statusFields = ["status", "state", "online", "deviceStatus"];
  for (const field of statusFields) {
    if (typeof data[field] === "string" && data[field]) {
      return String(data[field]).toLowerCase();
    }
  }
  if (data.online === true || data.online === "true") return "online";
  if (data.online === false || data.online === "false") return "offline";
  return "offline";
}

function resolveIcon(data: Record<string, unknown>): string {
  if (typeof data.icon === "string" && data.icon) return data.icon;
  const deviceType = String(data.deviceType || data.type || "").toLowerCase();
  if (deviceType && DEVICE_ICON_MAP[deviceType]) return DEVICE_ICON_MAP[deviceType];
  // Fuzzy match
  for (const [key, icon] of Object.entries(DEVICE_ICON_MAP)) {
    if (deviceType.includes(key)) return icon;
  }
  return "thermometer"; // default IoT icon
}

// ── Collapsible trigger ──
const PROP_COLLAPSE_THRESHOLD = 4;

// ── Main render ──
export function renderDeviceCard(
  data: Record<string, unknown>,
  onAction?: (fn: string, args: Record<string, unknown>) => void,
): TemplateResult {
  const deviceId = String(data.deviceId || data.id || "");
  const deviceName = String(data.displayName || data.name || data.deviceName || data.title || deviceId);
  const status = normalizeStatus(data);
  const properties = normalizeProperties(data);
  const telemetry = normalizeTelemetry(data);
  const primaryMetric = normalizePrimaryMetric(data, properties);
  const icon = resolveIcon(data);

  // Optional fields
  const signalStrength = typeof data.signalStrength === "number" ? data.signalStrength as number : undefined;
  const lastSeenRaw = data.lastSeen || data.updatedAt || data.lastUpdated;
  const lastSeen = lastSeenRaw ? formatRelativeTime(String(lastSeenRaw)) : undefined;
  const sparkline = data.sparkline as number[] | undefined;
  const actions = (data.actions as Array<{ label: string; functionId: string }>)?.length
    ? (data.actions as Array<{ label: string; functionId: string }>)
    : [
        { label: "查看详情", functionId: "viewDevice" },
        { label: "控制", functionId: "controlDevice" },
      ];
  const tags = (data.tags as string[])?.length
    ? (data.tags as string[])
    : (data.deviceType ? [String(data.deviceType)] : []);

  // Filter primaryMetric out of properties to avoid duplication
  const displayProperties = properties.filter(p =>
    !primaryMetric || (p.key !== primaryMetric.key && p.value !== primaryMetric.value)
  );

  const cfg = STATUS_CONFIG[status] || STATUS_CONFIG.offline;
  const showCollapse = displayProperties.length > PROP_COLLAPSE_THRESHOLD;

  console.log("[DeviceCard] Normalized — name:", deviceName, "status:", status,
    "primary:", primaryMetric, "props:", displayProperties.length, "telemetry:", telemetry.length);

  return html`
    <div class="a2ui-device-card a2ui-device-card--${status}"
         style="--status-color: ${cfg.color}; --status-glow: ${cfg.glow};">

      <!-- ══ Header ══ -->
      <div class="a2ui-device-card__header">
        <div class="a2ui-device-card__identity">
          <span class="a2ui-device-card__status ${cfg.pulse ? "a2ui-device-card__status--pulse" : ""}"></span>
          <span class="a2ui-device-card__icon" title="${icon}">
            <!-- SVG icon placeholder — rendered via icon system when available -->
            ${icon !== "thermometer" ? icon : html`<svg viewBox="0 0 20 20" xmlns="http://www.w3.org/2000/svg" fill="none" stroke="currentColor" stroke-width="1.5"><circle cx="7" cy="14" r="3"/><path d="M7 12V4a2 2 0 014 0v8" stroke-linecap="round"/></svg>`}
          </span>
          <span class="a2ui-device-card__name" title="${deviceName}">${deviceName}</span>
        </div>
        <div class="a2ui-device-card__header-right">
          ${signalStrength != null ? renderSignalBars(signalStrength) : nothing}
          <span class="a2ui-device-card__badge">${cfg.label}</span>
        </div>
      </div>

      <!-- ══ Primary Metric ══ -->
      ${primaryMetric ? html`
        <div class="a2ui-device-card__primary">
          <span class="a2ui-device-card__primary-key">${primaryMetric.key}</span>
          <div class="a2ui-device-card__primary-row">
            <span class="a2ui-device-card__primary-val">${primaryMetric.value}</span>
            ${primaryMetric.unit ? html`<span class="a2ui-device-card__primary-unit">${primaryMetric.unit}</span>` : nothing}
          </div>
        </div>
      ` : nothing}

      <!-- ══ Tags ══ -->
      ${tags.length ? html`
        <div class="a2ui-device-card__tags">
          ${tags.map((t) => html`<span class="a2ui-device-card__tag">${t}</span>`)}
        </div>
      ` : nothing}

      <!-- ══ Properties (collapsible) ══ -->
      ${displayProperties.length ? html`
        <div class="a2ui-device-card__props ${showCollapse ? "a2ui-device-card__props--collapsed" : ""}"
             id="props-${deviceId}">
          ${displayProperties.map((p) => html`
            <div class="a2ui-device-card__prop">
              <span class="a2ui-device-card__prop-key">${p.key}</span>
              <span class="a2ui-device-card__prop-val">
                ${p.value}${p.unit ? html`<span class="a2ui-device-card__prop-unit">${p.unit}</span>` : nothing}
              </span>
            </div>
          `)}
        </div>
        ${showCollapse ? html`
          <button class="a2ui-device-card__expand-btn"
                  @click=${(e: Event) => {
                    const btn = e.currentTarget as HTMLElement;
                    const el = btn.parentElement!.querySelector(".a2ui-device-card__props")!;
                    el.classList.toggle("a2ui-device-card__props--collapsed");
                    btn.textContent = el.classList.contains("a2ui-device-card__props--collapsed")
                      ? `展开全部 (${displayProperties.length})` : "收起";
                  }}>
            展开全部 (${displayProperties.length})
          </button>
        ` : nothing}
      ` : nothing}

      <!-- ══ Telemetry Grid ══ -->
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

      <!-- ══ Sparkline ══ -->
      ${sparkline?.length ? html`
        <div class="a2ui-device-card__sparkline">
          <span class="a2ui-device-card__sparkline-min">${Math.min(...sparkline)}</span>
          ${renderSparkline(sparkline, 120, 36, cfg.color)}
          <span class="a2ui-device-card__sparkline-max">${Math.max(...sparkline)}</span>
        </div>
      ` : nothing}

      <!-- ══ Footer ══ -->
      <div class="a2ui-device-card__footer">
        <div class="a2ui-device-card__footer-left">
          ${lastSeen ? html`<span class="a2ui-device-card__last-seen">
            <svg viewBox="0 0 12 12" xmlns="http://www.w3.org/2000/svg" fill="none"><circle cx="6" cy="6" r="4.5" stroke="currentColor" stroke-width="1"/><polyline points="6,3.5 6,6 8,7.5" stroke="currentColor" stroke-width="1" stroke-linecap="round" stroke-linejoin="round"/></svg>
            ${lastSeen}
          </span>` : nothing}
          <span class="a2ui-device-card__id" title="点击复制设备 ID"
                @click=${() => { navigator.clipboard.writeText(deviceId).then(() => {
                  // brief visual feedback handled by CSS :active
                }); }}>
            ${deviceId}
          </span>
        </div>
        ${actions.length ? html`
          <div class="a2ui-device-card__actions">
            ${actions.slice(0, 3).map((a) => html`
              <button class="a2ui-device-card__action"
                      @click=${(e: Event) => { e.stopPropagation(); if (onAction) onAction(a.functionId, { deviceId }); }}>
                ${a.label}
              </button>
            `)}
          </div>
        ` : nothing}
      </div>

      <!-- ══ Corner Glow ══ -->
      <div class="a2ui-device-card__corner-glow"></div>
    </div>
  `;
}
