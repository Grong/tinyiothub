import { html, nothing } from "lit";
import { repeat } from "lit/directives/repeat.js";
import type { AgentsState } from "../controllers/agents.js";

export interface HeartbeatConfig {
  enabled: boolean;
  intervalMinutes: number;
  workspaceId: string;
  agentId: string;
  tasks: HeartbeatTask[];
}

export interface HeartbeatTask {
  priority: string;
  text: string;
  paused: boolean;
}

export interface HeartbeatExecutionRecord {
  timestamp: string;
  taskCount: number;
  status: string;
  errorMessage?: string;
}

export interface HeartbeatLogsResponse {
  logs: HeartbeatExecutionRecord[];
}

export function renderHeartbeatTab(
  state: AgentsState,
  onToggleHeartbeat: (enabled: boolean) => void,
  onChangeInterval: (interval: number) => void,
  onToggleTask: (index: number, paused: boolean) => void,
  onAddTask: (task: HeartbeatTask) => void,
  onRemoveTask: (index: number) => void,
  onUpdateTask: (index: number, patch: Partial<HeartbeatTask>) => void,
) {
  const config = (state as any).heartbeatConfig as HeartbeatConfig | null;
  const logs = (state as any).heartbeatLogs as HeartbeatExecutionRecord[] | null;
  const loading = (state as any).heartbeatLoading as boolean | null;
  const error = (state as any).heartbeatError as string | null;

  if (loading && !config) {
    return html`
      <div class="heartbeat-tab">
        <div class="heartbeat-section">
          <div class="heartbeat-header-row">
            <div class="heartbeat-pulse-dot"></div>
            <h3>心跳配置</h3>
          </div>
          <div class="heartbeat-skeleton-row">
            <div class="heartbeat-skeleton skeleton-toggle"></div>
            <div class="heartbeat-skeleton skeleton-select"></div>
          </div>
        </div>
        <div class="heartbeat-section">
          <div class="heartbeat-header-row">
            <div class="heartbeat-pulse-dot"></div>
            <h3>执行历史</h3>
          </div>
          <div class="heartbeat-skeleton-list">
            ${[1, 2, 3].map(() => html`<div class="heartbeat-skeleton skeleton-row"></div>`)}
          </div>
        </div>
      </div>
    `;
  }

  if (error && !config) {
    return html`
      <div class="heartbeat-tab">
        <div class="heartbeat-section">
          <div class="heartbeat-empty-state">
            <svg class="heartbeat-empty-icon" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.5">
              <path d="M12 9v4m0 4h.01M21 12a9 9 0 11-18 0 9 9 0 0118 0z"/>
            </svg>
            <span>${error}</span>
          </div>
        </div>
      </div>
    `;
  }

  const intervals = [5, 15, 30, 60];

  return html`
    <div class="heartbeat-tab">
      <div class="heartbeat-section ${config?.enabled ? 'heartbeat-active' : ''}">
        <div class="heartbeat-header-row">
          <div class="heartbeat-pulse-dot ${config?.enabled ? 'active' : ''}"></div>
          <h3>心跳配置</h3>
        </div>

        <div class="heartbeat-controls">
          <label class="toggle-label">
            <input
              type="checkbox"
              class="toggle-input"
              .checked=${config?.enabled ?? false}
              @change=${(e: Event) => {
                const checked = (e.target as HTMLInputElement).checked;
                onToggleHeartbeat(checked);
              }}
            />
            <span class="toggle-track">
              <span class="toggle-thumb"></span>
            </span>
            <span class="toggle-text">${config?.enabled ? '运行中' : '已停止'}</span>
          </label>

          <label class="interval-label">
            <span class="interval-prefix">
              <svg viewBox="0 0 20 20" fill="currentColor" width="14" height="14">
                <path fill-rule="evenodd" d="M10 18a8 8 0 100-16 8 8 0 000 16zm1-12a1 1 0 10-2 0v4a1 1 0 00.293.707l2.828 2.829a1 1 0 101.415-1.415L11 9.586V6z" clip-rule="evenodd"/>
              </svg>
            </span>
            <select
              class="interval-select"
              .value=${String(config?.intervalMinutes ?? 30)}
              @change=${(e: Event) => {
                const value = parseInt((e.target as HTMLSelectElement).value, 10);
                onChangeInterval(value);
              }}
            >
              ${intervals.map(
                (mins) => html`
                  <option value=${mins} ?selected=${mins === (config?.intervalMinutes ?? 30)}>
                    ${mins} 分钟
                  </option>
                `
              )}
            </select>
          </label>
        </div>

        <div class="heartbeat-divider"></div>

        <div class="heartbeat-task-list">
          <div class="heartbeat-task-header">
            <svg viewBox="0 0 20 20" fill="currentColor" width="12" height="12">
              <path fill-rule="evenodd" d="M16.707 5.293a1 1 0 010 1.414l-8 8a1 1 0 01-1.414 0l-4-4a1 1 0 011.414-1.414L8 12.586l7.293-7.293a1 1 0 011.414 0z" clip-rule="evenodd"/>
            </svg>
            <span>巡检任务</span>
            <button
              class="task-add-btn"
              title="添加任务"
              @click=${() => {
                onAddTask({ priority: "medium", text: "新任务", paused: false });
              }}
            >
              <svg viewBox="0 0 20 20" fill="currentColor" width="12" height="12">
                <path fill-rule="evenodd" d="M10 3a1 1 0 011 1v5h5a1 1 0 110 2h-5v5a1 1 0 11-2 0v-5H4a1 1 0 110-2h5V4a1 1 0 011-1z" clip-rule="evenodd"/>
              </svg>
              添加
            </button>
          </div>
          <ul class="heartbeat-tasks">
            ${repeat(
              config?.tasks ?? [],
              (_task, index) => `task-${index}`,
              (task, index) => html`
                <li class="task-item ${task.paused ? 'paused' : ''}">
                  <button
                    class="task-pause-btn ${task.paused ? 'paused' : ''}"
                    title="${task.paused ? '恢复' : '暂停'}"
                    @click=${() => onToggleTask(index, !task.paused)}
                  >
                    ${task.paused
                      ? html`<svg viewBox="0 0 20 20" fill="currentColor" width="12" height="12"><path d="M6.3 2.841A1.5 1.5 0 004 4.11v11.78a1.5 1.5 0 002.3 1.269l9.344-5.89a1.5 1.5 0 000-2.538L6.3 2.84z"/></svg>`
                      : html`<svg viewBox="0 0 20 20" fill="currentColor" width="12" height="12"><path d="M5.75 3a.75.75 0 00-.75.75v12.5c0 .414.336.75.75.75h1.5a.75.75 0 00.75-.75V3.75A.75.75 0 007.25 3h-1.5zM12.75 3a.75.75 0 00-.75.75v12.5c0 .414.336.75.75.75h1.5a.75.75 0 00.75-.75V3.75a.75.75 0 00-.75-.75h-1.5z"/></svg>`
                    }
                  </button>
                  <select
                    class="task-priority-select ${task.priority}"
                    .value=${task.priority}
                    @change=${(e: Event) => {
                      const newPriority = (e.target as HTMLSelectElement).value;
                      onUpdateTask(index, { priority: newPriority });
                    }}
                  >
                    <option value="high" ?selected=${task.priority === "high"}>高优先级</option>
                    <option value="medium" ?selected=${task.priority === "medium"}>中优先级</option>
                    <option value="low" ?selected=${task.priority === "low"}>低优先级</option>
                  </select>
                  <input
                    type="text"
                    class="task-text-input"
                    .value=${task.text}
                    placeholder="描述任务..."
                    @change=${(e: Event) => {
                      const newText = (e.target as HTMLInputElement).value;
                      onUpdateTask(index, { text: newText });
                    }}
                  />
                  <button
                    class="task-remove-btn"
                    title="删除"
                    @click=${() => onRemoveTask(index)}
                  >
                    <svg viewBox="0 0 20 20" fill="currentColor" width="12" height="12">
                      <path fill-rule="evenodd" d="M4.293 4.293a1 1 0 011.414 0L10 8.586l4.293-4.293a1 1 0 111.414 1.414L11.414 10l4.293 4.293a1 1 0 01-1.414 1.414L10 11.414l-4.293 4.293a1 1 0 01-1.414-1.414L8.586 10 4.293 5.707a1 1 0 010-1.414z" clip-rule="evenodd"/>
                    </svg>
                  </button>
                </li>
              `
            )}
            ${(config?.tasks ?? []).length === 0
              ? html`<li class="task-empty">暂无巡检任务，点击"添加"创建</li>`
              : nothing}
          </ul>
        </div>
      </div>

      <div class="heartbeat-section">
        <div class="heartbeat-header-row">
          <div class="heartbeat-pulse-dot ${config?.enabled ? 'active' : ''}"></div>
          <h3>执行历史</h3>
          ${logs && logs.length > 0
            ? html`<span class="heartbeat-count">${logs.length} 条</span>`
            : nothing}
        </div>

        ${logs === null || logs.length === 0
          ? html`
              <div class="heartbeat-empty-state">
                <svg class="heartbeat-empty-icon" viewBox="0 0 48 48" fill="none" stroke="currentColor" stroke-width="1.5">
                  <circle cx="24" cy="24" r="20"/>
                  <path d="M24 14v10l6 6" stroke-linecap="round"/>
                </svg>
                <span>心跳未触发</span>
                <span class="heartbeat-empty-sub">下一个周期到达后将自动执行</span>
              </div>
            `
          : html`
              <div class="heartbeat-timeline">
                ${logs.map(
                  (log, index) => html`
                    <div class="heartbeat-timeline-item ${log.status}" style="--delay: ${index * 50}ms">
                      <div class="timeline-indicator">
                        <div class="timeline-dot ${log.status}"></div>
                        ${index < logs.length - 1 ? html`<div class="timeline-line"></div>` : nothing}
                      </div>
                      <div class="timeline-content">
                        <div class="timeline-header">
                          <span class="timeline-time">
                            ${new Date(log.timestamp).toLocaleString("zh-CN", {
                              month: "2-digit",
                              day: "2-digit",
                              hour: "2-digit",
                              minute: "2-digit",
                              second: "2-digit",
                            })}
                          </span>
                          <span class="timeline-badge ${log.status}">
                            ${log.status === "success" ? '成功' : '失败'}
                          </span>
                        </div>
                        <div class="timeline-meta">
                          <span class="timeline-tasks">
                            <svg viewBox="0 0 16 16" fill="currentColor" width="11" height="11">
                              <path d="M3 3.5a.5.5 0 01.5-.5H5a.5.5 0 010 1H3.5a.5.5 0 01-.5-.5zm0 2a.5.5 0 01.5-.5H7a.5.5 0 010 1H3.5a.5.5 0 01-.5-.5zm0 2a.5.5 0 01.5-.5H9a.5.5 0 010 1H3.5a.5.5 0 01-.5-.5zm0 2a.5.5 0 01.5-.5h1a.5.5 0 010 1H3.5a.5.5 0 01-.5-.5z"/>
                            </svg>
                            ${log.taskCount} 个任务
                          </span>
                          ${log.errorMessage
                            ? html`<span class="timeline-error" title="${log.errorMessage}">${log.errorMessage}</span>`
                            : nothing}
                        </div>
                      </div>
                    </div>
                  `
                )}
              </div>
            `}
      </div>
    </div>
  `;
}
