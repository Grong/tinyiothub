import { html, nothing, type TemplateResult } from "lit";
import type { AgentsState } from "../controllers/agents.js";

// Agent 预设模板
export const PERSONA_PRESETS = [
  {
    id: "ops",
    label: "运维助手",
    prompt: `你是 TinyIoTHub 的智能运维专家，负责监控和管理物联网网关上的所有设备。

## 核心职责
- 实时监控设备状态，及时发现异常
- 分析设备日志和告警，快速定位问题
- 执行设备维护操作（重启、固件升级）
- 优化设备配置以提升性能和稳定性

## 操作规范
- 读取操作优先于写入操作
- 批量操作前先确认影响范围
- 危险操作（如重启关键设备）需二次确认
- 修改配置前先备份当前配置`,
  },
  {
    id: "monitor",
    label: "监控助手",
    prompt: `你是 TinyIoTHub 的实时监控专家，负责持续监控 IoT 系统的运行状态。

## 核心职责
- 实时跟踪所有在线设备的状态变化
- 分析传感器数据趋势，识别潜在风险
- 及时生成告警并推送给相关人员
- 生成监控报告和趋势分析

## 数据处理原则
- 关注数据异常而非正常值
- 告警应简洁明了，包含可操作建议
- 历史数据用于趋势分析，不做实时告警`,
  },
  {
    id: "support",
    label: "客服助手",
    prompt: `你是 TinyIoTHub 的技术支持专家，为用户提供专业的设备管理和故障排除指导。

## 核心职责
- 解答用户关于设备使用的问题
- 引导用户完成常见配置任务
- 诊断并帮助解决设备连接问题
- 提供最佳实践建议和操作指南

## 服务规范
- 使用通俗易懂的语言解释技术概念
- 优先通过描述性步骤指导而非直接操作
- 复杂问题记录并升级给运维团队
- 保持耐心和专业的服务态度`,
  },
  {
    id: "custom",
    label: "自定义",
    prompt: "",
  },
];

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

  const isDirty = state.configDirty;

  return html`
    <section class="card">
      ${isDirty
        ? html`<div class="callout warn" style="margin-bottom: 16px;">
            有未保存的更改。
          </div>`
        : nothing}

      <div class="card-title">Agent 灵魂设定</div>
      <div class="card-sub">定义 Agent 的角色定位、行为规范和操作约束。</div>

      <div class="label" style="margin-top: 16px;">预设模板</div>
      <div class="agent-preset-cards" style="display: grid; grid-template-columns: repeat(auto-fill, minmax(140px, 1fr)); gap: 10px; margin-top: 10px; margin-bottom: 20px;">
        ${PERSONA_PRESETS.map(p => html`
          <div
            class="agent-preset-card ${config.personaPreset === p.id ? 'agent-preset-card--active' : ''}"
            @click=${() => {
              const prompt = p.prompt || config.systemPrompt || "";
              onStateChange({
                config: { ...config, personaPreset: p.id, systemPrompt: prompt },
                configDirty: true,
              });
            }}
          >
            <div class="agent-preset-card__label">${p.label}</div>
            ${p.id === "custom" ? html`<div class="agent-preset-card__desc">自定义提示词</div>` : nothing}
          </div>
        `)}
      </div>

      <div class="field">
        <div class="label" style="display: flex; justify-content: space-between;">
          <span>系统提示词</span>
          <span class="text-muted" style="font-size: 12px; font-weight: normal;">
            ${(config.systemPrompt || "").length} 字符
          </span>
        </div>
        <textarea
          class="textarea"
          rows="10"
          placeholder="输入 Agent 的系统提示词，定义其角色、职责和行为规范..."
          .value=${config.systemPrompt || ""}
          @input=${(e: InputEvent) => {
            onStateChange({
              config: {
                ...config,
                systemPrompt: (e.target as HTMLTextAreaElement).value,
                personaPreset: "custom",
              },
              configDirty: true,
            });
          }}
        ></textarea>
      </div>

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
