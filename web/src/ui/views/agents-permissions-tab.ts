import { html } from "lit";
import { apiGet, apiPut } from "../../api/client.js";
import { fetchAssistantSchedule, updateAssistantSchedule, type ScheduleSnapshot } from "../../api/assistant.js";

export type PermissionMode = "read_only" | "ask" | "full_access";

interface AutonomyPolicy {
  permissionMode: PermissionMode | string;
  allowedActions: string[];
  deniedActions: string[];
  maxActionsPerRun: number;
  maxActionsPerHour: number;
}

interface TabState {
  policy: AutonomyPolicy | null;
  schedule: ScheduleSnapshot | null;
  loading: boolean;
  saving: boolean;
  scheduleSaving: boolean;
  error: string;
  scheduleError: string;
  loadedWorkspaceId: string | null;
}

const tabStates = new Map<string, TabState>();

/// 巡检间隔候选（分钟）。后端允许 1..1440；UI 从 5 分钟起，避免巡检过密。
const INTERVAL_MINUTES = [5, 15, 30, 60, 120, 240, 720, 1440];

function intervalOptions(currentSeconds: number): number[] {
  const current = Math.round(currentSeconds / 60);
  return INTERVAL_MINUTES.includes(current) ? INTERVAL_MINUTES : [...INTERVAL_MINUTES, current].sort((a, b) => a - b);
}

function stateFor(workspaceId: string): TabState {
  let state = tabStates.get(workspaceId);
  if (!state || state.loadedWorkspaceId !== workspaceId) {
    state = {
      policy: null,
      schedule: null,
      loading: true,
      saving: false,
      scheduleSaving: false,
      error: "",
      scheduleError: "",
      loadedWorkspaceId: workspaceId,
    };
    tabStates.set(workspaceId, state);
  }
  return state;
}

async function loadPolicy(workspaceId: string): Promise<AutonomyPolicy> {
  const response = await apiGet<AutonomyPolicy>(`/workspaces/${workspaceId}/agent/policy`);
  return response.result ?? {
    permissionMode: "ask",
    allowedActions: ["*"],
    deniedActions: [],
    maxActionsPerRun: 3,
    maxActionsPerHour: 30,
  };
}

async function saveMode(workspaceId: string, mode: PermissionMode, onUpdate: () => void): Promise<void> {
  const state = stateFor(workspaceId);
  if (!state.policy || state.policy.permissionMode === mode) return;
  const previous = state.policy.permissionMode;
  state.policy = { ...state.policy, permissionMode: mode };
  state.saving = true;
  state.error = "";
  onUpdate();
  try {
    const response = await apiPut<AutonomyPolicy>(`/workspaces/${workspaceId}/agent/policy`, state.policy);
    state.policy = response.result ?? state.policy;
  } catch (error) {
    state.policy = { ...state.policy, permissionMode: previous };
    state.error = error instanceof Error ? error.message : String(error);
  } finally {
    state.saving = false;
    onUpdate();
  }
}

async function saveScheduleEnabled(workspaceId: string, enabled: boolean, onUpdate: () => void): Promise<void> {
  const state = stateFor(workspaceId);
  if (!state.schedule || state.schedule.enabled === enabled) return;
  const previous = state.schedule.enabled;
  state.schedule = { ...state.schedule, enabled };
  state.scheduleSaving = true;
  state.scheduleError = "";
  onUpdate();
  try {
    state.schedule = await updateAssistantSchedule(workspaceId, { enabled });
  } catch (error) {
    state.schedule = { ...state.schedule, enabled: previous };
    state.scheduleError = error instanceof Error ? error.message : String(error);
  } finally {
    state.scheduleSaving = false;
    onUpdate();
  }
}

async function saveScheduleInterval(workspaceId: string, minutes: number, onUpdate: () => void): Promise<void> {
  const state = stateFor(workspaceId);
  const seconds = minutes * 60;
  if (!state.schedule || state.schedule.intervalSeconds === seconds) return;
  const previous = state.schedule.intervalSeconds;
  state.schedule = { ...state.schedule, intervalSeconds: seconds };
  state.scheduleSaving = true;
  state.scheduleError = "";
  onUpdate();
  try {
    state.schedule = await updateAssistantSchedule(workspaceId, { intervalSeconds: seconds });
  } catch (error) {
    state.schedule = { ...state.schedule, intervalSeconds: previous };
    state.scheduleError = error instanceof Error ? error.message : String(error);
  } finally {
    state.scheduleSaving = false;
    onUpdate();
  }
}

function formatWhen(value: string | null): string {
  if (!value) return "尚未运行";
  const date = new Date(value);
  return Number.isNaN(date.getTime()) ? value : date.toLocaleString();
}

export function renderPermissionsTab(workspaceId: string, onUpdate: () => void) {
  const state = stateFor(workspaceId);
  if (state.loading) {
    Promise.all([loadPolicy(workspaceId), fetchAssistantSchedule(workspaceId)])
      .then(([policy, schedule]) => {
        state.policy = policy;
        state.schedule = schedule;
        state.loading = false;
        onUpdate();
      })
      .catch((error) => {
        state.error = error instanceof Error ? error.message : String(error);
        state.loading = false;
        onUpdate();
      });
    return html`<div class="callout info">加载权限与巡检配置...</div>`;
  }
  if (!state.schedule) {
    return html`<div class="callout error">${state.error || "加载巡检配置失败"}</div>`;
  }

  const schedule = state.schedule;
  const mode = state.policy?.permissionMode ?? "ask";
  const intervalMinutes = Math.round(schedule.intervalSeconds / 60);
  return html`
    <section class="card">
      <div class="card-title">定时巡检</div>
      <div class="card-sub">
        AI 大脑按此间隔唤醒并执行巡检。间隔与「全局权限」共同决定巡检能做什么；暂停后不再产生新的巡检 run。
      </div>

      <div class="field" style="margin-top: 16px;">
        <label class="field-label" style="display: flex; align-items: center; gap: 8px;">
          <input
            type="checkbox"
            .checked=${schedule.enabled}
            ?disabled=${state.scheduleSaving}
            @change=${(event: Event) =>
              saveScheduleEnabled(workspaceId, (event.target as HTMLInputElement).checked, onUpdate)}
          />
          <span>${schedule.enabled ? "运行中" : "已暂停"}</span>
        </label>
      </div>

      <div class="field">
        <label class="field-label" for="agent-inspection-interval">巡检间隔</label>
        <select
          id="agent-inspection-interval"
          class="select"
          .value=${String(intervalMinutes)}
          ?disabled=${state.scheduleSaving}
          @change=${(event: Event) =>
            saveScheduleInterval(workspaceId, Number((event.target as HTMLSelectElement).value), onUpdate)}
        >
          ${intervalOptions(schedule.intervalSeconds).map(
            (minutes) => html`
              <option value=${minutes} ?selected=${minutes === intervalMinutes}>${minutes} 分钟</option>
            `,
          )}
        </select>
      </div>

      <div class="callout info" style="margin-top: 12px;">
        上次巡检：${formatWhen(schedule.lastWakeAt)} · 下次巡检：${formatWhen(schedule.nextWakeAt)}
      </div>

      ${state.scheduleError
        ? html`<div class="callout error" style="margin-top: 12px;">${state.scheduleError}</div>`
        : ""}
    </section>

    <section class="card" style="margin-top: 16px;">
      <div class="card-title">全局权限</div>
      <div class="card-sub">
        统一控制 Chat、定时巡检和 Story 事件唤醒。完全访问只免除逐次审批，不绕过急停、联锁和审计。
      </div>

      <div class="field" style="margin-top: 16px;">
        <label class="field-label" for="agent-permission-mode">权限模式</label>
        <select
          id="agent-permission-mode"
          class="select"
          .value=${mode}
          ?disabled=${state.saving}
          @change=${(event: Event) =>
            saveMode(workspaceId, (event.target as HTMLSelectElement).value as PermissionMode, onUpdate)}
        >
          <option value="read_only" ?selected=${mode === "read_only"}>只读</option>
          <option value="ask" ?selected=${mode === "ask"}>默认审批</option>
          <option value="full_access" ?selected=${mode === "full_access"}>完全权限</option>
        </select>
      </div>

      <div class="callout info" style="margin-top: 12px;">
        ${
          mode === "read_only"
            ? "只允许查询；所有设备和动作写入都会被拒绝。"
            : mode === "full_access"
              ? "写入和动作不再逐次等待人工确认；急停、联锁、Effect 和审计仍然保留。"
              : "写入和动作需要人工确认；适合有人值守的默认模式。"
        }
      </div>

      ${state.error ? html`<div class="callout error" style="margin-top: 12px;">${state.error}</div>` : ""}
    </section>
  `;
}
