import { html, type TemplateResult } from "lit";

export function renderA2uiDateTimeInput(data: Record<string, unknown>, onAction?: (fn: string, args: Record<string, unknown>) => void): TemplateResult {
  const label = String(data.label || "");
  const value = String(data.value || "");
  const inputType = String(data.inputType || "datetime-local");
  const disabled = Boolean(data.disabled);
  const functionId = data.functionId as string | undefined;

  return html`
    <div class="a2ui-date-time ${disabled ? "a2ui-date-time--disabled" : ""}">
      ${label ? html`<label class="a2ui-date-time__label">${label}</label>` : ""}
      <div class="a2ui-date-time__wrap">
        <input
          class="a2ui-date-time__input"
          type=${inputType}
          .value=${value}
          ?disabled=${disabled}
          @change=${(e: Event) => {
            const el = e.target as HTMLInputElement;
            if (functionId && onAction) onAction(functionId, { value: el.value });
          }}
        />
        <span class="a2ui-date-time__glow"></span>
      </div>
    </div>
  `;
}
