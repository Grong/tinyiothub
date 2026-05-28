import { html, type TemplateResult } from "lit";

export function renderA2uiTextField(data: Record<string, unknown>, onAction?: (fn: string, args: Record<string, unknown>) => void): TemplateResult {
  const label = String(data.label || "");
  const value = String(data.value || "");
  const placeholder = String(data.placeholder || "");
  const type = String(data.inputType || "text");
  const disabled = Boolean(data.disabled);
  const functionId = data.functionId as string | undefined;

  return html`
    <div class="a2ui-text-field ${disabled ? "a2ui-text-field--disabled" : ""}">
      ${label ? html`<label class="a2ui-text-field__label">${label}</label>` : ""}
      <div class="a2ui-text-field__wrap">
        <input
          class="a2ui-text-field__input"
          type=${type}
          .value=${value}
          placeholder=${placeholder}
          ?disabled=${disabled}
          @input=${(e: InputEvent) => {
            const el = e.target as HTMLInputElement;
            if (functionId && onAction) onAction(functionId, { value: el.value });
          }}
        />
        <span class="a2ui-text-field__glow"></span>
      </div>
    </div>
  `;
}
