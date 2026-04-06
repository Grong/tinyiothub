import { html, type TemplateResult } from "lit";
import type { AgentsState } from "../controllers/agents.js";

export function renderModelTab(
  state: AgentsState,
  onStateChange: (patch: Partial<AgentsState>) => void,
  onSave: () => void,
  onReload: () => void,
): TemplateResult {
  const config = state.config;
  if (state.configLoading) {
    return html`<div class="agent-panel-loading">加载中...</div>`;
  }
  if (!config) {
    return html`<div class="agent-panel-empty">未找到配置</div>`;
  }

  const models: string[] = config.alternativeModels?.length
    ? config.alternativeModels
    : [config.model || "default"];
  const currentModel = config.model || models[0];

  return html`
    <div class="agent-model-tab">
      <div class="agent-field">
        <label class="agent-field__label">模型</label>
        <select class="agent-model-dropdown"
                .value=${currentModel}
                @change=${(e: Event) => {
                  onStateChange({
                    config: { ...config, model: (e.target as HTMLSelectElement).value },
                    configDirty: true,
                  });
                }}>
          ${models.map((m) => html`<option value=${m} ?selected=${m === currentModel}>${m}</option>`)}
        </select>
      </div>

      <div class="agent-field">
        <label class="agent-field__label">Temperature</label>
        <div class="agent-slider-row">
          <input type="range" class="agent-slider" min="0" max="2" step="0.1"
                 .value=${String(config.temperature ?? 1.0)}
                 @input=${(e: Event) => {
                   onStateChange({
                     config: { ...config, temperature: parseFloat((e.target as HTMLInputElement).value) },
                     configDirty: true,
                   });
                 }} />
          <span class="agent-slider-value">${((config.temperature ?? 1.0) as number).toFixed(1)}</span>
        </div>
      </div>

      <div class="agent-field">
        <label class="agent-field__label">System Prompt</label>
        <textarea class="agent-system-prompt" rows="6"
                  .value=${config.systemPrompt || ""}
                  @input=${(e: Event) => {
                    onStateChange({
                      config: { ...config, systemPrompt: (e.target as HTMLTextAreaElement).value },
                      configDirty: true,
                    });
                  }}></textarea>
      </div>

      <div class="agent-actions">
        <button class="btn btn-primary" ?disabled=${!state.configDirty}
                @click=${onSave}>
          保存${state.configDirty ? " *" : ""}
        </button>
        <button class="btn" @click=${onReload}>重新加载</button>
      </div>
    </div>
  `;
}
