import { LitElement, html, nothing } from "lit";
import { customElement, state } from "lit/decorators.js";
import type { AgentsState, AgentsPanel } from "../controllers/agents.js";
import { createAgentsState, loadAgents, loadAgentConfig, saveAgentConfig, loadToolsCatalog, toggleTool, loadSkills, loadHeartbeatConfig, loadHeartbeatLogs, updateHeartbeatConfig, updateHeartbeatTasks } from "../controllers/agents.js";
import { renderModelTab } from "./agents-model-tab.js";
import { renderToolsTab } from "./agents-tools-tab.js";
import { renderSkillsTab } from "./agents-skills-tab.js";
import { renderHeartbeatTab, type HeartbeatTask } from "./agents-heartbeat-tab.js";

const panelLabels: Record<AgentsPanel, string> = {
  overview: "配置",
  tools: "工具权限",
  skills: "技能",
  heartbeat: "心跳",
};

@customElement("view-agents")
export class ViewAgents extends LitElement {
  @state() state: AgentsState = createAgentsState();

  createRenderRoot() {
    return this;
  }

  connectedCallback(): void {
    super.connectedCallback();
    Promise.all([
      loadAgents(this.state),
      loadSkills(this.state),
    ]).then(() => {
      this.requestUpdate();
      if (this.state.selectedAgentId) {
        this.onAgentSelected(this.state.selectedAgentId);
      }
    });
  }

  private onAgentSelected(agentId: string): void {
    this.state = { ...this.state, selectedAgentId: agentId, activePanel: "overview" };
    Promise.all([
      loadAgentConfig(this.state, agentId),
      loadToolsCatalog(this.state, agentId),
      loadSkills(this.state),
      loadHeartbeatConfig(this.state, agentId),
      loadHeartbeatLogs(this.state, agentId),
    ]).then(() => this.requestUpdate());
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

  private async onToggleHeartbeat(enabled: boolean): Promise<void> {
    if (!this.state.selectedAgentId) return;
    await updateHeartbeatConfig(this.state, this.state.selectedAgentId, enabled, undefined);
    await loadHeartbeatLogs(this.state, this.state.selectedAgentId);
    this.requestUpdate();
  }

  private async onChangeHeartbeatInterval(intervalMinutes: number): Promise<void> {
    if (!this.state.selectedAgentId) return;
    await updateHeartbeatConfig(this.state, this.state.selectedAgentId, undefined, intervalMinutes);
    await loadHeartbeatLogs(this.state, this.state.selectedAgentId);
    this.requestUpdate();
  }

  private async onToggleHeartbeatTask(index: number, paused: boolean): Promise<void> {
    if (!this.state.selectedAgentId || !this.state.heartbeatConfig) return;
    const tasks = [...(this.state.heartbeatConfig.tasks || [])];
    tasks[index] = { ...tasks[index], paused };
    await updateHeartbeatTasks(this.state, this.state.selectedAgentId, tasks);
    await loadHeartbeatLogs(this.state, this.state.selectedAgentId);
    this.requestUpdate();
  }

  private async onAddHeartbeatTask(task: { priority: string; text: string; paused: boolean }): Promise<void> {
    if (!this.state.selectedAgentId || !this.state.heartbeatConfig) return;
    const tasks = [...(this.state.heartbeatConfig.tasks || []), task];
    await updateHeartbeatTasks(this.state, this.state.selectedAgentId, tasks);
    await loadHeartbeatLogs(this.state, this.state.selectedAgentId);
    this.requestUpdate();
  }

  private async onRemoveHeartbeatTask(index: number): Promise<void> {
    if (!this.state.selectedAgentId || !this.state.heartbeatConfig) return;
    const task = this.state.heartbeatConfig.tasks?.[index];
    if (!confirm(`确定要删除任务"${task?.text || ''}"吗？`)) return;
    const tasks = (this.state.heartbeatConfig.tasks || []).filter((_, i) => i !== index);
    await updateHeartbeatTasks(this.state, this.state.selectedAgentId, tasks);
    await loadHeartbeatLogs(this.state, this.state.selectedAgentId);
    this.requestUpdate();
  }

  private async onUpdateHeartbeatTask(index: number, patch: Partial<HeartbeatTask>): Promise<void> {
    if (!this.state.selectedAgentId || !this.state.heartbeatConfig) return;
    const tasks = [...(this.state.heartbeatConfig.tasks || [])];
    tasks[index] = { ...tasks[index], ...patch };
    await updateHeartbeatTasks(this.state, this.state.selectedAgentId, tasks);
    await loadHeartbeatLogs(this.state, this.state.selectedAgentId);
    this.requestUpdate();
  }

  private _patchState(patch: Partial<AgentsState>): void {
    this.state = { ...this.state, ...patch };
  }

  render(): ReturnType<typeof html> {
    const agents = this.state.agentsList?.agents || [];

    if (this.state.agentsLoading && !agents.length) {
      return html`<div class="agents-layout"><div class="agent-panel-loading">加载 Agent 列表...</div></div>`;
    }
    if (this.state.agentsError && !agents.length) {
      return html`<div class="agents-layout"><div class="agent-panel-error">${this.state.agentsError}</div></div>`;
    }

    const allPanels: AgentsPanel[] = ["overview", "tools", "skills", "heartbeat"];

    // Tools tab search — local state to avoid re-render on other panels
    const searchFilter = (this as any)._searchFilter || "";
    const setSearchFilter = (v: string) => { (this as any)._searchFilter = v; this.requestUpdate(); };

    return html`
      <div class="agents-layout">
        <div class="agents-header">
          <div class="agents-selector">
            <select class="agent-dropdown"
                    @change=${(e: Event) => this.onAgentSelected((e.target as HTMLSelectElement).value)}>
              ${agents.map((a) => html`
                <option value=${a.id} ?selected=${a.id === this.state.selectedAgentId}>
                  ${a.name || a.id}
                </option>
              `)}
            </select>
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

        <div class="agents-main">
          ${this.state.activePanel === "overview" ? renderModelTab(this.state, this._patchState.bind(this), this.onSaveConfig.bind(this), () => { if (this.state.selectedAgentId) loadAgentConfig(this.state, this.state.selectedAgentId).then(() => this.requestUpdate()); }) : nothing}
          ${this.state.activePanel === "tools" ? renderToolsTab(this.state, searchFilter, setSearchFilter, this.onToggleTool.bind(this)) : nothing}
          ${this.state.activePanel === "skills" ? renderSkillsTab(
            this.state,
            this._patchState.bind(this),
            () => { if (this.state.selectedAgentId) loadSkills(this.state).then(() => this.requestUpdate()); }
          ) : nothing}
          ${this.state.activePanel === "heartbeat" ? renderHeartbeatTab(
            this.state,
            this.onToggleHeartbeat.bind(this),
            this.onChangeHeartbeatInterval.bind(this),
            this.onToggleHeartbeatTask.bind(this),
            this.onAddHeartbeatTask.bind(this),
            this.onRemoveHeartbeatTask.bind(this),
            this.onUpdateHeartbeatTask.bind(this)
          ) : nothing}
        </div>
      </div>
    `;
  }
}
