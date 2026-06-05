import { html, nothing, type TemplateResult } from "lit";

type StatItem = {
  label?: string;
  value?: string | number;
  unit?: string;
  description?: string;
  color?: string;
};

export function renderStatRow(
  data: Record<string, unknown>,
  _onAction?: (fn: string, args: Record<string, unknown>) => void,
): TemplateResult {
  const items = (data.items as StatItem[]) || [];
  const columns = data.columns as number | undefined;

  if (items.length === 0) {
    return html`<div class="a2ui-stat-row a2ui-stat-row--empty"></div>`;
  }

  const gridStyle = columns
    ? `grid-template-columns: repeat(${columns}, 1fr);`
    : `grid-template-columns: repeat(${items.length}, 1fr);`;

  return html`
    <div class="a2ui-stat-row" style=${gridStyle}>
      ${items.map(
        (item) => html`
          <div class="a2ui-stat-row__item">
            ${item.label
              ? html`<div class="a2ui-stat-row__label">${item.label}</div>`
              : nothing}
            <div class="a2ui-stat-row__value-wrap">
              <span class="a2ui-stat-row__value">${item.value ?? "—"}</span>
              ${item.unit
                ? html`<span class="a2ui-stat-row__unit">${item.unit}</span>`
                : nothing}
            </div>
            ${item.description
              ? html`<div class="a2ui-stat-row__desc">${item.description}</div>`
              : nothing}
          </div>
        `,
      )}
    </div>
  `;
}
