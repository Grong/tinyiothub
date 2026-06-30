import { html, nothing, type TemplateResult } from "lit";
import { safeStr } from "./utils.js";

export function renderDataChart(
  data: Record<string, unknown>,
  _onAction?: (fn: string, args: Record<string, unknown>) => void,
): TemplateResult {
  const title = safeStr(data.title, "图表");
  const unit = safeStr(data.unit, "");
  const timeRange = safeStr(data.timeRange, "1h");
  const series = (data.series as Array<{
    name: string;
    color: string;
    data: Array<{ time: string; value: number }>;
  }>) || [];
  const thresholds = (data.thresholds as Array<{ label: string; value: number; color: string }>) || [];

  const width = 360;
  const height = 160;
  const padLeft = 36;
  const padRight = 12;
  const padTop = 12;
  const padBottom = 24;
  const chartW = width - padLeft - padRight;
  const chartH = height - padTop - padBottom;

  const allValues = series.flatMap((s) => s.data.map((d) => d.value));
  const allThresholdValues = thresholds.map((t) => t.value);
  const yMin = Math.min(0, ...allValues, ...allThresholdValues);
  const yMax = Math.max(1, ...allValues, ...allThresholdValues) * 1.1;
  const yRange = yMax - yMin || 1;

  function toX(i: number, total: number): number {
    return padLeft + (i / (total - 1 || 1)) * chartW;
  }
  function toY(v: number): number {
    return padTop + chartH - ((v - yMin) / yRange) * chartH;
  }

  const seriesPolylines = series.map((s) => {
    const points = s.data
      .map((d, i) => `${toX(i, s.data.length).toFixed(1)},${toY(d.value).toFixed(1)}`)
      .join(" ");
    return { name: s.name, color: s.color, points };
  });

  const yTicks = [0, 0.25, 0.5, 0.75, 1].map((p) => ({
    value: yMin + p * yRange,
    y: toY(yMin + p * yRange),
  }));

  const firstSeries = series[0];
  const xLabels: Array<{ label: string; x: number }> = [];
  if (firstSeries?.data.length) {
    const len = firstSeries.data.length;
    const indices = [0, Math.floor(len / 2), len - 1];
    for (const i of indices) {
      const t = firstSeries.data[i]?.time;
      if (t) {
        xLabels.push({
          label: new Date(t).toLocaleTimeString([], { hour: "numeric", minute: "2-digit" }),
          x: toX(i, len),
        });
      }
    }
  }

  return html`
    <div class="a2ui-data-chart">
      <div class="a2ui-data-chart__header">
        <span class="a2ui-data-chart__title">${title}</span>
        <span class="a2ui-data-chart__range">${timeRange}</span>
      </div>

      <svg width="100%" viewBox="0 0 ${width} ${height}" xmlns="http://www.w3.org/2000/svg"
           class="a2ui-data-chart__svg">
        ${yTicks.map((t) => html`
          <line x1=${padLeft} y1=${t.y.toFixed(1)} x2=${width - padRight} y2=${t.y.toFixed(1)}
                stroke="var(--border)" stroke-width="0.5" />
        `)}

        ${yTicks.map((t) => html`
          <text x=${padLeft - 4} y=${(t.y + 3).toFixed(1)} text-anchor="end"
                fill="var(--muted)" font-size="10">
            ${t.value.toFixed(0)}${unit ? ` ${unit}` : ""}
          </text>
        `)}

        ${thresholds.map((th) => html`
          <line x1=${padLeft} y1=${toY(th.value).toFixed(1)}
                x2=${width - padRight} y2=${toY(th.value).toFixed(1)}
                stroke=${th.color} stroke-width="1" stroke-dasharray="4 2" />
          <text x=${width - padRight - 2} y=${(toY(th.value) - 4).toFixed(1)}
                text-anchor="end" fill=${th.color} font-size="9">${th.label}</text>
        `)}

        ${seriesPolylines.map((s) => html`
          <polyline points=${s.points} fill="none" stroke=${s.color}
                    stroke-width="2" stroke-linecap="round" stroke-linejoin="round" />
        `)}

        ${xLabels.map((l) => html`
          <text x=${l.x.toFixed(1)} y=${height - 4} text-anchor="middle"
                fill="var(--muted)" font-size="10">${l.label}</text>
        `)}
      </svg>

      ${series.length > 1 ? html`
        <div class="a2ui-data-chart__legend">
          ${series.map((s) => html`
            <span class="a2ui-data-chart__legend-item">
              <span class="a2ui-data-chart__legend-dot" style="background: ${s.color}"></span>
              ${s.name}
            </span>
          `)}
        </div>
      ` : nothing}
    </div>
  `;
}
