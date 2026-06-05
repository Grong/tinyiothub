import { html, type TemplateResult } from "lit";

export function renderA2uiSlider(data: Record<string, unknown>, onAction?: (fn: string, args: Record<string, unknown>) => void): TemplateResult {
  const label = String(data.label || "");
  const value = Number(data.value ?? 50);
  const min = Number(data.min ?? 0);
  const max = Number(data.max ?? 100);
  const step = Number(data.step ?? 1);
  const disabled = Boolean(data.disabled);
  const functionId = data.functionId as string | undefined;
  const showValue = Boolean(data.showValue ?? true);
  const unit = String(data.unit || "");

  const pct = max > min ? ((value - min) / (max - min)) * 100 : 0;

  return html`
    <div class="a2ui-slider ${disabled ? "a2ui-slider--disabled" : ""}">
      ${label ? html`<label class="a2ui-slider__label">${label}</label>` : ""}
      <div class="a2ui-slider__row">
        <div class="a2ui-slider__track">
          <div class="a2ui-slider__fill" style="width:${pct}%"></div>
          <input
            class="a2ui-slider__input"
            type="range"
            .value=${String(value)}
            min=${min}
            max=${max}
            step=${step}
            ?disabled=${disabled}
            @input=${(e: InputEvent) => {
              const el = e.target as HTMLInputElement;
              if (functionId && onAction) onAction(functionId, { value: Number(el.value) });
            }}
          />
        </div>
        ${showValue ? html`<span class="a2ui-slider__value">${value}${unit}</span>` : ""}
      </div>
    </div>
  `;
}
