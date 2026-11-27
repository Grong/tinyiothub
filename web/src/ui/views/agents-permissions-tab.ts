import { html } from "lit";
import { apiGet, apiPut } from "../../api/client.js";

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
  loading: boolean;
  saving: boolean;
  error: string;
  loadedWorkspaceId: string | null;
}

const tabStates = new Map<string, TabState>();

function stateFor(workspaceId: string): TabState {
  let state = tabStates.get(workspaceId);
  if (!state || state.loadedWorkspaceId !== workspaceId) {
    state = { policy: null, loading: true, saving: false, error: "", loadedWorkspaceId: workspaceId };
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

export function renderPermissionsTab(workspaceId: string, onUpdate: () => void) {
  const state = stateFor(workspaceId);
  if (state.loading) {
    loadPolicy(workspaceId)
      .then((policy) => {
        state.policy = policy;
        state.loading = false;
        onUpdate();
      })
      .catch((error) => {
        state.error = error instanceof Error ? error.message : String(error);
        state.loading = false;
        onUpdate();
      });
    return html`<div class="callout info">加载权限配置...</div>`;
  }

  const mode = state.policy?.permissionMode ?? "ask";
  return html`
    <section class="card">
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
