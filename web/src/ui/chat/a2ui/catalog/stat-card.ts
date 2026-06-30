import { html, nothing, type TemplateResult } from "lit";
import { safeStr } from "./utils.js";

export function renderStatCard(
  data: Record<string, unknown>,
  onAction?: (fn: string, args: Record<string, unknown>) => void,
): TemplateResult {
  const label = safeStr(data.label, "指标");
  const value = safeStr(data.value, "0");
  const unit = safeStr(data.unit, "");
  const trend = data.trend as number | undefined;
  const trendLabel = data.trendLabel as string | undefined;
  const color = safeStr(data.color, "accent");
  const icon = data.icon as string | undefined;
  const description = data.description as string | undefined;
  const actions = (data.actions as Array<{ label: string; functionId: string }>) || [];

  const isPositiveTrend = (trend ?? 0) >= 0;
  const trendColor = isPositiveTrend ? "var(--success)" : "var(--danger)";

  return html`
    <div class="a2ui-stat-card a2ui-stat-card--${color}" style="--card-accent: var(--${color});">
      <div class="a2ui-stat-card__inner">
        <!-- Header -->
        <div class="a2ui-stat-card__header">
          <span class="a2ui-stat-card__label">${label}</span>
          ${icon ? html`<span class="a2ui-stat-card__icon">${icon}</span>` : nothing}
        </div>

        <!-- Value Display -->
        <div class="a2ui-stat-card__body">
          <span class="a2ui-stat-card__value">${value}</span>
          ${unit ? html`<span class="a2ui-stat-card__unit">${unit}</span>` : nothing}
        </div>

        <!-- Trend Indicator -->
        ${trend != null ? html`
          <div class="a2ui-stat-card__trend" style="color: ${trendColor}">
            <span class="a2ui-stat-card__trend-arrow">${isPositiveTrend ? '↑' : '↓'}</span>
            <span class="a2ui-stat-card__trend-val">${Math.abs(trend)}%</span>
            ${trendLabel ? html`<span class="a2ui-stat-card__trend-label">${trendLabel}</span>` : nothing}
          </div>
        ` : nothing}

        <!-- Description -->
        ${description ? html`
          <div class="a2ui-stat-card__desc">${description}</div>
        ` : nothing}

        <!-- Actions -->
        ${actions.length ? html`
          <div class="a2ui-stat-card__actions">
            ${actions.map((a) => html`
              <button class="a2ui-btn a2ui-btn--ghost a2ui-btn--xs"
                      @click=${() => { if (onAction) onAction(a.functionId, {}); }}>
                ${a.label}
              </button>
            `)}
          </div>
        ` : nothing}
      </div>

      <!-- Decorative Glow -->
      <div class="a2ui-stat-card__glow"></div>
    </div>
  `;
}
