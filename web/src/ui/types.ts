// UI-specific types for agent-related components

export interface ToolCatalogEntry {
  id: string;
  label?: string;
  description?: string;
  enabled?: boolean;
  source?: string;
  pluginId?: string;
  optional?: boolean;
}

export interface ToolCatalogGroup {
  label?: string;
  source?: string;
  pluginId?: string;
  tools?: ToolCatalogEntry[];
}

export interface AgentItem {
  id: string;
  name?: string;
}

export interface AgentsListResult {
  agents: AgentItem[];
}
