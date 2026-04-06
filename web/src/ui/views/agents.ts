import { LitElement, html, nothing } from "lit";
import { customElement, state } from "lit/decorators.js";
import type { AgentsState, AgentsPanel, AgentConfig } from "../controllers/agents.js";
import { createAgentsState, loadAgents, loadAgentConfig, saveAgentConfig, loadToolsCatalog, toggleTool } from "../controllers/agents.js";

const panelLabels: Record<AgentsPanel, string> = {
  overview: "概览",
  files: "文件",
  tools: "工具",
  skills: "技能",
  channels: "渠道",
  cron: "定时任务",
};

@customElement("view-agents")
export class ViewAgents extends LitElement {
  @state() state: AgentsState = createAgentsState();
  @state() searchFilter: string = "";

  createRenderRoot() {
    return this;
  }

  connectedCallback(): void {
    super.connectedCallback();
    loadAgents(this.state).then(() => {
      if (this.state.selectedAgentId) {
        this.onAgentSelected(this.state.selectedAgentId);
      }
    });
  }

  private onAgentSelected(agentId: string): void {
    this.state = { ...this.state, selectedAgentId: agentId, activePanel: "overview" };
    loadAgentConfig(this.state, agentId);
    loadToolsCatalog(this.state, agentId);
  }

  private async onSaveConfig(): Promise<void> {
    if (!this.state.selectedAgentId) return;
    const ok = await saveAgentConfig(this.state, this.state.selectedAgentId);
    if (ok) {
      this.requestUpdate();
    }
  }

  private async onToggleTool(toolName: string, enabled: boolean): Promise<void> {
    if (!this.state.selectedAgentId) return;
    await toggleTool(this.state.selectedAgentId, toolName, enabled);
    await loadToolsCatalog(this.state, this.state.selectedAgentId);
    this.requestUpdate();
  }

  private renderOverview(): ReturnType<typeof html> {
    const config = this.state.config;
    if (this.state.configLoading) {
      return html`<div class="agent-panel-loading">加载中...</div>`;
    }
    if (!config) {
      return html`<div class="agent-panel-empty">未找到配置</div>`;
    }

    const models = ["claude-sonnet-4-5", "claude-haiku-4-5", "gpt-4o", "gpt-4o-mini"];
    const currentModel = config.model || models[0];

    return html`
      <div class="agent-overview">
        <div class="agent-field">
          <label>模型</label>
          <div class="agent-model-chips">
            ${models.map((m) => html`
              <button class="agent-chip ${m === currentModel ? 'active' : ''}"
                      @click=${() => {
                        this.state = {
                          ...this.state,
                          config: { ...config, model: m },
                          configDirty: true,
                        };
                      }}>
                ${m}
              </button>
            `)}
          </div>
        </div>
        <div class="agent-field">
          <label>工作空间</label>
          <span class="agent-value">${config.workspace || "默认"}</span>
        </div>
        <div class="agent-actions">
          <button class="btn btn-primary" ?disabled=${!this.state.configDirty}
                  @click=${() => this.onSaveConfig()}>
            保存${this.state.configDirty ? " *" : ""}
          </button>
          <button class="btn" @click=${() => {
            if (this.state.selectedAgentId) loadAgentConfig(this.state, this.state.selectedAgentId);
          }}>重新加载</button>
        </div>
      </div>
    `;
  }

  private renderTools(): ReturnType<typeof html> {
    if (this.state.toolsCatalogLoading) {
      return html`<div class="agent-panel-loading">加载工具目录...</div>`;
    }
    if (!this.state.toolsCatalog?.length) {
      return html`<div class="agent-panel-empty">暂无可用工具</div>`;
    }

    return html`
      <div class="agent-tools">
        ${this.state.toolsCatalog.map((group) => html`
          <div class="agent-tool-group">
            <h4 class="agent-tool-group__title">${group.label || group.name}</h4>
            <div class="agent-tool-list">
              ${(group.tools || []).map((tool: Record<string, unknown>) => html`
                <div class="agent-tool-item">
                  <div class="agent-tool-info">
                    <span class="agent-tool-name">${tool.name as string}</span>
                    <span class="agent-tool-desc">${(tool.description as string) || ""}</span>
                  </div>
                  <label class="agent-toggle">
                    <input type="checkbox"
                           ?checked=${tool.enabled as boolean}
                           @change=${(e: Event) => this.onToggleTool(tool.name as string, (e.target as HTMLInputElement).checked)} />
                    <span class="agent-toggle__slider"></span>
                  </label>
                </div>
              `)}
            </div>
          </div>
        `)}
      </div>
    `;
  }

  private renderPlaceholder(title: string): ReturnType<typeof html> {
    return html`
      <div class="agent-panel-placeholder">
        <span class="agent-placeholder-icon">⚡</span>
        <p>${title} — 即将推出</p>
      </div>
    `;
  }

  render(): ReturnType<typeof html> {
    const agents = this.state.agentsList?.agents || [];

    if (this.state.agentsLoading && !agents.length) {
      return html`<div class="agents-layout"><div class="agent-panel-loading">加载 Agent 列表...</div></div>`;
    }
    if (this.state.agentsError && !agents.length) {
      return html`<div class="agents-layout"><div class="agent-panel-error">${this.state.agentsError}</div></div>`;
    }

    const allPanels: AgentsPanel[] = ["overview", "files", "tools", "skills", "channels", "cron"];

    return html`
      <div class="agents-layout">
        <div class="agents-header">
          <h2>Agent 管理</h2>
          <div class="agents-selector">
            ${agents.map((a) => html`
              <button class="agent-pill ${a.id === this.state.selectedAgentId ? 'active' : ''}"
                      @click=${() => this.onAgentSelected(a.id)}>
                ${a.name || a.id}
              </button>
            `)}
          </div>
        </div>

        <div class="agent-tabs">
          ${allPanels.map((panel) => html`
            <button class="agent-tab ${this.state.activePanel === panel ? 'active' : ''}"
                    @click=${() => { this.state = { ...this.state, activePanel: panel }; }}>
              ${panelLabels[panel]}
            </button>
          `)}
        </div>

        <div class="agent-panel-content">
          ${this.state.activePanel === "overview" ? this.renderOverview() : nothing}
          ${this.state.activePanel === "tools" ? this.renderTools() : nothing}
          ${this.state.activePanel === "files" ? this.renderPlaceholder("文件管理") : nothing}
          ${this.state.activePanel === "skills" ? this.renderPlaceholder("技能管理") : nothing}
          ${this.state.activePanel === "channels" ? this.renderPlaceholder("渠道管理") : nothing}
          ${this.state.activePanel === "cron" ? this.renderPlaceholder("定时任务") : nothing}
        </div>
      </div>
    `;
  }
}
