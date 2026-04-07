import { html, nothing, type TemplateResult } from "lit";
import type { AgentsState } from "../controllers/agents.js";

export function renderModelTab(
  state: AgentsState,
  onStateChange: (patch: Partial<AgentsState>) => void,
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

  const agent = state.agentsList?.agents.find(a => a.id === state.selectedAgentId);
  const workspace = agent?.workspace || config.workspace || "default";
  const primaryModel = config.model || agent?.model || "-";
  const fallbacks: string[] = config.alternativeModels || [];
  const isDirty = state.configDirty;

  const removeFallback = (index: number) => {
    const next = fallbacks.filter((_, i) => i !== index);
    onStateChange({ config: { ...config, alternativeModels: next }, configDirty: true });
  };

  const addFallback = (value: string) => {
    const trimmed = value.trim();
    if (!trimmed || fallbacks.includes(trimmed)) return;
    onStateChange({
      config: { ...config, alternativeModels: [...fallbacks, trimmed] },
      configDirty: true,
    });
  };

  const handleChipKeydown = (e: KeyboardEvent) => {
    if (e.key === "Enter" || e.key === ",") {
      e.preventDefault();
      const input = e.target as HTMLInputElement;
      addFallback(input.value);
      input.value = "";
    }
  };

  const handleChipBlur = (e: Event) => {
    const input = e.target as HTMLInputElement;
    addFallback(input.value);
    input.value = "";
  };

  const updateModel = (value: string) => {
    onStateChange({ config: { ...config, model: value || undefined }, configDirty: true });
  };

  return html`
    <section class="card">
      <div class="card-title">概览</div>
      <div class="card-sub">工作区路径和模型配置。</div>

      <div class="agents-overview-grid" style="margin-top: 16px;">
        <div class="agent-kv">
          <div class="label">工作区</div>
          <div class="mono">${workspace}</div>
        </div>
        <div class="agent-kv">
          <div class="label">主模型</div>
          <div class="mono">${primaryModel}</div>
        </div>
        <div class="agent-kv">
          <div class="label">备用模型</div>
          <div>${fallbacks.length > 0 ? `${fallbacks.length} 个` : "无"}</div>
        </div>
      </div>

      ${isDirty
        ? html`<div class="callout warn" style="margin-top: 16px;">
            有未保存的更改。
          </div>`
        : nothing}

      <div class="agent-model-select" style="margin-top: 20px;">
        <div class="label">模型选择</div>
        <div class="agent-model-fields">
          <div class="field">
            <span>主模型</span>
            <select
              class="select"
              .value=${config.model || ""}
              @change=${(e: Event) => updateModel((e.target as HTMLSelectElement).value)}
            >
              <option value="">默认 (${agent?.model || "无"})</option>
              ${(config.alternativeModels || [config.model]).filter(Boolean).map((m) => html`
                <option value=${m}>${m}</option>
              `)}
            </select>
          </div>
          <div class="field">
            <span>备用模型 (按优先级排序)</span>
            <div
              class="agent-chip-input"
              @click=${(e: Event) => {
                const container = e.currentTarget as HTMLElement;
                const input = container.querySelector("input");
                if (input) input.focus();
              }}
            >
              ${fallbacks.map((chip, i) => html`
                <span class="chip">
                  ${chip}
                  <button
                    type="button"
                    class="chip-remove"
                    @click=${() => removeFallback(i)}
                  >
                    &times;
                  </button>
                </span>
              `)}
              <input
                placeholder=${fallbacks.length === 0 ? "输入模型名称后按回车添加，如 anthropic/claude-3-5-sonnet" : ""}
                @keydown=${handleChipKeydown}
                @blur=${handleChipBlur}
              />
            </div>
          </div>
        </div>

        <div class="agent-model-actions" style="display: flex; gap: 8px; margin-top: 8px; flex-wrap: wrap;">
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
      </div>
    </section>
  `;
}
