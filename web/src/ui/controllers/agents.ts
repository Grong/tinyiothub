import { apiGet, apiPost, apiPut } from "../../api/client.js";
import type { AgentsListResult, ToolCatalogGroup } from "../types.js";

export type AgentsPanel = "overview" | "files" | "tools" | "skills" | "channels" | "cron";

export type AgentConfig = {
  model?: string;
  alternativeModels?: string[];
  workspace?: string;
  skills?: string[];
  tools?: {
    profile?: string;
    allow?: string[];
    alsoAllow?: string[];
    deny?: string[];
  };
  [key: string]: unknown;
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
