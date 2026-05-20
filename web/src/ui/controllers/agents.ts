import { apiGet, apiPost, apiPut } from "../../api/client.js";
import type { AgentsListResult, ToolCatalogGroup } from "../types.js";
import type { Skill, CreateSkillRequest, UpdateSkillRequest } from "../../api/client.js";
import { listSkills, createSkill, updateSkill, deleteSkill } from "../../api/client.js";
import type { HeartbeatConfig, HeartbeatLogsResponse, HeartbeatTask } from "../views/agents-heartbeat-tab.js";

export type AgentsPanel = "overview" | "tools" | "skills" | "heartbeat";

export type AgentConfig = {
  model?: string;
  alternativeModels?: string[];
  workspace?: string;
  skills?: string[];
  // ZeroClaw 层
  temperature?: number;
  maxTokens?: number;
  topP?: number;
  enableReflection?: boolean;
  tools?: {
    profile?: string;
    allow?: string[];
    alsoAllow?: string[];
    deny?: string[];
  };
};

export type AgentsState = {
  agentsLoading: boolean;
  agentsError: string | null;
  agentsList: AgentsListResult | null;
  selectedAgentId: string | null;
  activePanel: AgentsPanel;
  config: AgentConfig | null;
  configLoading: boolean;
  configDirty: boolean;
  configBaseHash: string | null;
  toolsCatalog: ToolCatalogGroup[] | null;
  toolsCatalogLoading: boolean;
  skillsList?: Skill[];
  skillsLoading?: boolean;
  skillsError?: string | null;
  activeSkillsPanel?: string;
  editingSkill?: Skill | null;
  skillDraft?: string;
  pendingDelete?: string | null;
  heartbeatConfig?: HeartbeatConfig | null;
  heartbeatLogs?: HeartbeatLogsResponse["logs"];
  heartbeatLoading?: boolean;
  heartbeatError?: string | null;
};

export function createAgentsState(): AgentsState {
  return {
    agentsLoading: false,
    agentsError: null,
    agentsList: null,
    selectedAgentId: null,
    activePanel: "overview",
    config: null,
    configLoading: false,
    configDirty: false,
    configBaseHash: null,
    toolsCatalog: null,
    toolsCatalogLoading: false,
  };
}

export async function loadAgents(state: AgentsState): Promise<void> {
  state.agentsLoading = true;
  state.agentsError = null;
  try {
    const res = await apiGet<AgentsListResult>("/agents");
    state.agentsList = res.result || null;
    if (state.agentsList?.agents?.length && !state.selectedAgentId) {
      state.selectedAgentId = state.agentsList.agents[0].id;
    }
  } catch (err) {
    state.agentsError = String(err);
  } finally {
    state.agentsLoading = false;
  }
}

export async function loadAgentConfig(state: AgentsState, agentId: string): Promise<void> {
  state.configLoading = true;
  try {
    const res = await apiGet<{ config: AgentConfig; baseHash?: string }>(`/agents/${agentId}/config`);
    state.config = res.result?.config || null;
    state.configBaseHash = res.result?.baseHash || null;
    state.configDirty = false;
  } catch (err) {
    state.agentsError = String(err);
  } finally {
    state.configLoading = false;
  }
}

export async function saveAgentConfig(state: AgentsState, agentId: string): Promise<boolean> {
  if (!state.config) return false;
  try {
    await apiPut(`/agents/${agentId}/config`, {
      config: state.config,
      baseHash: state.configBaseHash,
    });
    state.configDirty = false;
    return true;
  } catch (err) {
    state.agentsError = String(err);
    return false;
  }
}

export async function loadToolsCatalog(state: AgentsState, agentId: string): Promise<void> {
  state.toolsCatalogLoading = true;
  try {
    const res = await apiGet<{ groups: ToolCatalogGroup[] }>("/tools/catalog", { agentId });
    state.toolsCatalog = res.result?.groups || null;
  } catch (err) {
    state.agentsError = String(err);
  } finally {
    state.toolsCatalogLoading = false;
  }
}

export async function toggleTool(agentId: string, toolName: string, enabled: boolean): Promise<void> {
  await apiPost("/tools/toggle", { agentId, toolName, enabled });
}

export async function loadSkills(state: AgentsState): Promise<void> {
  state.skillsLoading = true;
  state.skillsError = null;
  try {
    const res = await listSkills();
    state.skillsList = res.result || [];
  } catch (err) {
    state.skillsError = String(err);
  } finally {
    state.skillsLoading = false;
  }
}

export async function saveSkill(state: AgentsState, skill: CreateSkillRequest, name?: string): Promise<boolean> {
  try {
    if (name) {
      await updateSkill(name, { skill_content: skill.skill_content }, skill.workspace_id);
    } else {
      await createSkill(skill);
    }
    await loadSkills(state);
    return true;
  } catch (err) {
    state.skillsError = String(err);
    return false;
  }
}

export async function removeSkill(state: AgentsState, name: string, workspaceId?: string): Promise<boolean> {
  try {
    await deleteSkill(name, workspaceId);
    await loadSkills(state);
    return true;
  } catch (err) {
    state.skillsError = String(err);
    return false;
  }
}

export async function createSkillApi(state: AgentsState, data: CreateSkillRequest): Promise<boolean> {
  try {
    await createSkill(data);
    await loadSkills(state);
    return true;
  } catch (err) {
    state.skillsError = String(err);
    return false;
  }
}

export async function updateSkillApi(state: AgentsState, name: string, data: UpdateSkillRequest, workspaceId?: string): Promise<boolean> {
  try {
    await updateSkill(name, data, workspaceId);
    await loadSkills(state);
    return true;
  } catch (err) {
    state.skillsError = String(err);
    return false;
  }
}

export async function loadHeartbeatConfig(state: AgentsState, agentId: string): Promise<void> {
  state.heartbeatLoading = true;
  state.heartbeatError = null;
  try {
    const res = await apiGet<HeartbeatConfig>(`/agents/${agentId}/heartbeat/config`);
    if (res.result) {
      // Parse tasks if it's a JSON string (same as templates/devices pattern)
      if (typeof res.result.tasks === "string") {
        res.result.tasks = JSON.parse(res.result.tasks);
      }
    }
    state.heartbeatConfig = res.result || null;
  } catch (err) {
    state.heartbeatError = String(err);
  } finally {
    state.heartbeatLoading = false;
  }
}

export async function loadHeartbeatLogs(state: AgentsState, agentId: string): Promise<void> {
  try {
    const res = await apiGet<HeartbeatLogsResponse>(`/agents/${agentId}/heartbeat/logs`);
    state.heartbeatLogs = res.result?.logs ?? undefined;
  } catch (err) {
    state.heartbeatError = String(err);
  }
}

export async function updateHeartbeatConfig(
  state: AgentsState,
  agentId: string,
  enabled?: boolean,
  intervalMinutes?: number
): Promise<boolean> {
  try {
    await apiPut(`/agents/${agentId}/heartbeat/config`, {
      enabled,
      intervalMinutes,
    });
    // Reload config to get the latest state
    await loadHeartbeatConfig(state, agentId);
    return true;
  } catch (err) {
    state.heartbeatError = String(err);
    return false;
  }
}

export async function updateHeartbeatTasks(
  state: AgentsState,
  agentId: string,
  tasks: HeartbeatTask[]
): Promise<boolean> {
  try {
    await apiPut(`/agents/${agentId}/heartbeat/tasks`, { tasks });
    // Reload config to get the updated tasks list
    await loadHeartbeatConfig(state, agentId);
    return true;
  } catch (err) {
    state.heartbeatError = String(err);
    return false;
  }
}

