import { html, type TemplateResult } from "lit";

export function renderProgressIndicator(data: Record<string, unknown>): TemplateResult {
  const value = Number(data.value || 0);
  const max = Number(data.max || 100);
  const pct = max > 0 ? Math.round((value / max) * 100) : 0;
  return html`
    <div class="a2ui-progress">
      <div style="background: var(--bg-subtle); border-radius: 4px; height: 8px; overflow: hidden;">
        <div style="width: ${pct}%; height: 100%; background: var(--accent); transition: width 0.3s;"></div>
      </div>
      <span class="a2ui-caption">${pct}%</span>
    </div>
  `;
}
