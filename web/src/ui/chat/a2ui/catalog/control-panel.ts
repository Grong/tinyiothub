import { html, type TemplateResult } from "lit";
import { safeStr } from "./utils.js";

export function renderControlPanel(
  data: Record<string, unknown>,
  onAction?: (fn: string, args: Record<string, unknown>) => void,
): TemplateResult {
  const deviceName = safeStr(data.deviceName || data.deviceId, "");
  const controls = (data.controls as Array<Record<string, unknown>>) || [];

  return html`
    <div class="a2ui-control-panel">
      <div class="a2ui-control-panel__header">控制面板: ${deviceName}</div>

      ${controls.map((ctrl) => {
        const type = safeStr(ctrl.type, "button");
        const label = safeStr(ctrl.label || ctrl.id, "");
        const id = safeStr(ctrl.id, "");

        if (type === "slider") {
          const min = Number(ctrl.min ?? 0);
          const max = Number(ctrl.max ?? 100);
          const step = Number(ctrl.step ?? 1);
          const value = Number(ctrl.value ?? min);
          const unit = safeStr(ctrl.unit, "");

          return html`
            <div class="a2ui-control-panel__field">
              <label class="a2ui-control-panel__label">${label}</label>
              <div class="a2ui-control-panel__slider-row">
                <span class="a2ui-control-panel__range-label">${min}${unit}</span>
                <input type="range" class="a2ui-control-panel__slider"
                       min=${min} max=${max} step=${step} value=${value}
                       @change=${(e: Event) => {
                         if (onAction) onAction(id, { value: parseFloat((e.target as HTMLInputElement).value) });
                       }} />
                <span class="a2ui-control-panel__range-label">${max}${unit}</span>
              </div>
            </div>
          `;
        }

        if (type === "toggle") {
          const checked = Boolean(ctrl.checked);

          return html`
            <div class="a2ui-control-panel__field">
              <label class="a2ui-control-panel__toggle">
                <input type="checkbox"
                       ?checked=${checked}
                       @change=${(e: Event) => {
                         if (onAction) onAction(id, { checked: (e.target as HTMLInputElement).checked });
                       }} />
                <span class="a2ui-control-panel__toggle-slider"></span>
                <span class="a2ui-control-panel__toggle-label">${label}</span>
              </label>
            </div>
          `;
        }

        if (type === "choice") {
          const options = (ctrl.options as Array<{ label: string; value: string }>) || [];
          const selected = safeStr(ctrl.selected, "");

          return html`
            <div class="a2ui-control-panel__field">
              <label class="a2ui-control-panel__label">${label}</label>
              <div class="a2ui-control-panel__choices">
                ${options.map((opt) => html`
                  <label class="a2ui-control-panel__choice">
                    <input type="radio" name=${id} value=${opt.value}
                           ?checked=${opt.value === selected}
                           @change=${() => { if (onAction) onAction(id, { value: opt.value }); }} />
                    <span>${opt.label}</span>
                  </label>
                `)}
              </div>
            </div>
          `;
        }

        // Default: button
        const variant = safeStr(ctrl.variant, "primary");
        const confirmMsg = safeStr(ctrl.confirmMessage, "");

        return html`
          <div class="a2ui-control-panel__field">
            <button class="a2ui-btn a2ui-btn--${variant}"
                    @click=${() => {
                      if (confirmMsg && !confirm(confirmMsg)) return;
                      if (onAction) onAction(id, {});
                    }}>
              ${label}
            </button>
          </div>
        `;
      })}
    </div>
  `;
}
