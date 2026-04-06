import { html, nothing, type TemplateResult } from "lit";
import type { AgentsState } from "../controllers/agents.js";

const DANGEROUS_TOOLS = new Set(["device_delete", "workspace_delete", "agent_delete", "batch_delete"]);

export function renderToolsTab(
  state: AgentsState,
  searchFilter: string,
  onSearchChange: (v: string) => void,
  onToggleTool: (name: string, enabled: boolean) => void,
): TemplateResult {
  if (state.toolsCatalogLoading) {
    return html`<div class="agent-panel-loading">加载工具目录...</div>`;
  }
  if (!state.toolsCatalog?.length) {
    return html`<div class="agent-panel-empty">暂无可用工具</div>`;
  }

  const filter = searchFilter.toLowerCase();

  return html`
    <div class="agent-tools-tab">
      <div class="agent-tools-toolbar">
        <input type="text" class="agent-tools-search" placeholder="搜索工具..."
               .value=${searchFilter}
               @input=${(e: Event) => onSearchChange((e.target as HTMLInputElement).value)} />
        <button class="btn btn-sm" @click=${() => {
          for (const g of state.toolsCatalog || []) {
            for (const t of (g.tools || []) as Record<string, unknown>[]) {
              onToggleTool(t.name as string, true);
            }
          }
        }}>全部启用</button>
        <button class="btn btn-sm" @click=${() => {
          for (const g of state.toolsCatalog || []) {
            for (const t of (g.tools || []) as Record<string, unknown>[]) {
              onToggleTool(t.name as string, false);
            }
          }
        }}>全部禁用</button>
      </div>

      ${state.toolsCatalog.map((group) => {
        const tools = ((group.tools || []) as Record<string, unknown>[]).filter((t) =>
          !filter || (t.name as string).toLowerCase().includes(filter) || ((t.description as string) || "").toLowerCase().includes(filter)
        );
        if (!tools.length) return nothing;

        return html`
          <div class="agent-tool-group">
            <h4 class="agent-tool-group__title">${group.label || group.name}</h4>
            <div class="agent-tool-list">
              ${tools.map((tool) => html`
                <div class="agent-tool-item ${DANGEROUS_TOOLS.has(tool.name as string) ? 'agent-tool-item--danger' : ''}">
                  <div class="agent-tool-info">
                    <span class="agent-tool-name">${tool.name as string}</span>
                    <span class="agent-tool-desc">${(tool.description as string) || ""}</span>
                  </div>
                  <label class="agent-toggle">
                    <input type="checkbox"
                           ?checked=${tool.enabled as boolean}
                           @change=${(e: Event) => onToggleTool(tool.name as string, (e.target as HTMLInputElement).checked)} />
                    <span class="agent-toggle__slider"></span>
                  </label>
                </div>
              `)}
            </div>
          </div>
        `;
      })}
    </div>
  `;
}
