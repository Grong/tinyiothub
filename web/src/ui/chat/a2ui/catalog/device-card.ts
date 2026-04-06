import { html, type TemplateResult } from "lit";

export function renderDeviceCard(data: Record<string, unknown>): TemplateResult {
  const deviceId = String(data.deviceId || "");
  const deviceName = String(data.deviceName || deviceId);
  const status = String(data.status || "unknown");
  const isOnline = status === "online";

  return html`
    <div class="a2ui-device-card">
      <div class="a2ui-device-card__header">
        <span class="a2ui-device-card__status ${isOnline ? 'online' : 'offline'}"></span>
        <span class="a2ui-device-card__name">${deviceName}</span>
      </div>
      <div class="a2ui-device-card__id">${deviceId}</div>
    </div>
  `;
}
