import { html, nothing, type TemplateResult } from "lit";
import type { AgentsState } from "../controllers/agents.js";
import type { ToolCatalogEntry, ToolCatalogGroup } from "../types.js";

const DANGEROUS_TOOLS = new Set(["device_delete", "workspace_delete", "agent_delete", "batch_delete"]);

export function renderToolsTab(
  state: AgentsState,
  searchFilter: string,
  onSearchChange: (v: string) => void,
  onToggleTool: (name: string, enabled: boolean) => void,
): TemplateResult {
  if (state.toolsCatalogLoading) {
    return html`<div class="callout info">加载工具目录...</div>`;
  }

  const groups = state.toolsCatalog || [];
  const filter = searchFilter.toLowerCase();
  const toolEntries: Array<{ section: ToolCatalogGroup; tool: ToolCatalogEntry }> = [];

  let totalCount = 0;
  let filteredCount = 0;
  for (const group of groups) {
    for (const tool of group.tools || [] as ToolCatalogEntry[]) {
      totalCount++;
      if (!filter || (tool.label || tool.id || "").toLowerCase().includes(filter) || (tool.description || "").toLowerCase().includes(filter)) {
        filteredCount++;
        toolEntries.push({ section: group, tool });
      }
    }
  }

  if (!groups.length) {
    return html`<div class="callout info">暂无可用工具</div>`;
  }

  return html`
    <section class="card">
      <div class="row" style="justify-content: space-between; flex-wrap: wrap;">
        <div style="min-width: 0;">
          <div class="card-title">工具访问</div>
          <div class="card-sub">
            为当前 Agent 配置可使用的工具。
            <span class="mono">${filteredCount}/${totalCount}</span> 个工具已加载。
          </div>
        </div>
        <div class="row" style="gap: 8px; flex-wrap: wrap;">
          <button class="btn btn--sm" @click=${() => {
            for (const { tool } of toolEntries) {
              onToggleTool(tool.id, true);
            }
          }}>全部启用</button>
          <button class="btn btn--sm" @click=${() => {
            for (const { tool } of toolEntries) {
              onToggleTool(tool.id, false);
            }
          }}>全部禁用</button>
          <button
            class="btn btn--sm primary"
            ?disabled=${state.configLoading || !state.configDirty}
          >
            ${state.configLoading ? "保存中..." : "保存配置"}
          </button>
        </div>
      </div>

      ${state.configDirty
        ? html`<div class="callout info" style="margin-top: 12px;">
            有未保存的更改。
          </div>`
        : nothing}

      <div class="filters" style="margin-top: 14px;">
        <div class="field" style="flex: 1;">
          <input
            type="text"
            .value=${searchFilter}
            @input=${(e: Event) => onSearchChange((e.target as HTMLInputElement).value)}
            placeholder="搜索工具名称或描述..."
          />
        </div>
        <span class="muted">${filteredCount} 个结果</span>
      </div>

      <div class="agent-tools-grid" style="margin-top: 20px;">
        ${groups.map((group) => {
          const filtered = (group.tools || []).filter((t: ToolCatalogEntry) =>
            !filter || (t.label || t.id || "").toLowerCase().includes(filter) || (t.description || "").toLowerCase().includes(filter)
          );
          if (!filtered.length) return nothing;

          return html`
            <div class="agent-tools-section">
              <div class="agent-tools-header">
                ${group.label}
                ${group.source === "plugin" && group.pluginId
                  ? html`<span class="agent-pill" style="margin-left: 8px;">plugin:${group.pluginId}</span>`
                  : nothing}
              </div>
              <div class="agent-tools-list">
                ${filtered.map((tool) => {
                  const enabled = tool.enabled ?? true;
                  return html`
                    <div class="agent-tool-row ${DANGEROUS_TOOLS.has(tool.id) ? 'agent-tool-item--danger' : ''}">
                      <div>
                        <div class="agent-tool-title mono">${tool.label || tool.id}</div>
                        <div class="agent-tool-sub">${tool.description || ""}</div>
                        <div style="display: flex; gap: 6px; flex-wrap: wrap; margin-top: 6px;">
                          ${tool.source === "plugin" ? html`<span class="agent-pill">plugin:${tool.pluginId || ""}</span>` : nothing}
                          ${tool.source === "core" ? html`<span class="agent-pill">core</span>` : nothing}
                          ${tool.optional ? html`<span class="agent-pill muted">optional</span>` : nothing}
                          ${DANGEROUS_TOOLS.has(tool.id) ? html`<span class="agent-pill warn">dangerous</span>` : nothing}
                        </div>
                      </div>
                      <label class="cfg-toggle">
                        <input
                          type="checkbox"
                          .checked=${enabled}
                          @change=${(e: Event) => onToggleTool(tool.id, (e.target as HTMLInputElement).checked)}
                        />
                        <span class="cfg-toggle__track"></span>
                      </label>
                    </div>
                  `;
                })}
              </div>
            </div>
          `;
        })}
      </div>
    </section>
  `;
}
