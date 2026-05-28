import { html, type TemplateResult } from "lit";

export function renderA2uiChoicePicker(data: Record<string, unknown>, onAction?: (fn: string, args: Record<string, unknown>) => void): TemplateResult {
  const label = String(data.label || "");
  const choices = (data.choices as Array<{ value: string; label: string }>) || [];
  const selectedValue = String(data.selectedValue || "");
  const functionId = data.functionId as string | undefined;
  const variant = String(data.variant || "radio");

  if (variant === "select") {
    return html`
      <div class="a2ui-choice-picker">
        ${label ? html`<label class="a2ui-choice-picker__label">${label}</label>` : ""}
        <div class="a2ui-choice-picker__select-wrap">
          <select
            class="a2ui-choice-picker__select"
            @change=${(e: Event) => {
              const el = e.target as HTMLSelectElement;
              if (functionId && onAction) onAction(functionId, { value: el.value });
            }}
          >
            ${choices.map((c) => html`
              <option value=${c.value} ?selected=${c.value === selectedValue}>${c.label}</option>
            `)}
          </select>
          <span class="a2ui-choice-picker__chevron">
            <svg viewBox="0 0 10 6" xmlns="http://www.w3.org/2000/svg"><path d="M1 1l4 4 4-4" stroke="currentColor" stroke-width="1.2" fill="none" stroke-linecap="round" stroke-linejoin="round"/></svg>
          </span>
        </div>
      </div>
    `;
  }

  return html`
    <div class="a2ui-choice-picker">
      ${label ? html`<div class="a2ui-choice-picker__label">${label}</div>` : ""}
      <div class="a2ui-choice-picker__options">
        ${choices.map((c) => html`
          <label class="a2ui-choice-picker__option ${c.value === selectedValue ? "a2ui-choice-picker__option--selected" : ""}">
            <input
              type="radio"
              name=${functionId || "choice"}
              value=${c.value}
              .checked=${c.value === selectedValue}
              @change=${() => { if (functionId && onAction) onAction(functionId, { value: c.value }); }}
            />
            <span class="a2ui-choice-picker__radio-mark"></span>
            <span class="a2ui-choice-picker__radio-label">${c.label}</span>
          </label>
        `)}
      </div>
    </div>
  `;
}
