import { html, type TemplateResult } from "lit";

export function renderA2uiCheckBox(data: Record<string, unknown>, onAction?: (fn: string, args: Record<string, unknown>) => void): TemplateResult {
  const label = String(data.label || "");
  const checked = Boolean(data.checked);
  const disabled = Boolean(data.disabled);
  const functionId = data.functionId as string | undefined;

  return html`
    <label class="a2ui-check-box ${disabled ? "a2ui-check-box--disabled" : ""}">
      <input
        class="a2ui-check-box__input"
        type="checkbox"
        .checked=${checked}
        ?disabled=${disabled}
        @change=${(e: Event) => {
          const el = e.target as HTMLInputElement;
          if (functionId && onAction) onAction(functionId, { checked: el.checked });
        }}
      />
      <span class="a2ui-check-box__mark">
        <svg class="a2ui-check-box__check" viewBox="0 0 10 8" xmlns="http://www.w3.org/2000/svg">
          <path d="M1 4l2.5 2.5L9 1" stroke="currentColor" stroke-width="1.5" fill="none" stroke-linecap="round" stroke-linejoin="round"/>
        </svg>
      </span>
      ${label ? html`<span class="a2ui-check-box__label">${label}</span>` : ""}
    </label>
  `;
}
