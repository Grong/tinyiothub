import { html, type TemplateResult } from "lit";

export function renderProgressIndicator(data: Record<string, unknown>): TemplateResult {
  const label = String(data.label || "");
  const value = Number(data.value ?? 0);
  const max = Number(data.max ?? 100);
  const pct = max > 0 ? Math.min(Math.round((value / max) * 100), 100) : 0;
  const variant = String(data.variant || "linear");
  const color = String(data.color || "accent");

  if (variant === "circular") {
    const r = 18;
    const c = 2 * Math.PI * r;
    const dash = (pct / 100) * c;
    return html`
      <div class="a2ui-progress a2ui-progress--circular a2ui-progress--${color}">
        <svg viewBox="0 0 44 44" xmlns="http://www.w3.org/2000/svg">
          <circle cx="22" cy="22" r=${r} fill="none" stroke="var(--bg-muted)" stroke-width="3"/>
          <circle cx="22" cy="22" r=${r} fill="none" stroke="currentColor" stroke-width="3"
            stroke-dasharray="${dash} ${c - dash}" stroke-linecap="round"
            transform="rotate(-90 22 22)" style="transition: stroke-dasharray 0.4s var(--ease-out)"/>
        </svg>
        <span class="a2ui-progress__label">${label ? html`${label}<br/>` : ""}${pct}%</span>
      </div>
    `;
  }

  return html`
    <div class="a2ui-progress a2ui-progress--linear a2ui-progress--${color}">
      ${label ? html`<div class="a2ui-progress__header"><span>${label}</span><span>${pct}%</span></div>` : ""}
      <div class="a2ui-progress__track">
        <div class="a2ui-progress__fill" style="width:${pct}%"></div>
        <div class="a2ui-progress__glow" style="left:${pct}%"></div>
      </div>
    </div>
  `;
}
