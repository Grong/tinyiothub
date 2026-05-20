import { html, nothing, type TemplateResult } from "lit";
import type { AgentsState } from "../controllers/agents.js";

export function renderModelTab(
  state: AgentsState,
  _onStateChange: (patch: Partial<AgentsState>) => void,
  onSave: () => void,
  onReload: () => void,
): TemplateResult {
  const config = state.config;

  if (state.configLoading) {
    return html`<div class="callout info">加载配置...</div>`;
  }
  if (!config) {
    return html`<div class="callout info">未找到配置</div>`;
  }

  const isDirty = state.configDirty;

  return html`
    <section class="card">
      ${isDirty
        ? html`<div class="callout warn" style="margin-bottom: 16px;">
            有未保存的更改。
          </div>`
        : nothing}

      <div class="card-title">模型配置</div>

      <div class="agent-model-actions" style="display: flex; gap: 8px; margin-top: 16px; flex-wrap: wrap;">
        <button
          type="button"
          class="btn btn--sm"
          ?disabled=${state.configLoading}
          @click=${onReload}
        >
          重新加载
        </button>
        <button
          type="button"
          class="btn btn--sm primary"
          ?disabled=${!isDirty}
          @click=${onSave}
        >
          保存配置
        </button>
      </div>
    </section>
  `;
}
