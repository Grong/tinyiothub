export type ToolViewStatus = 'running' | 'ok' | 'failed';

export interface ToolMetric {
  label: string;
  value: string;
}

export interface ToolDetail {
  label: string;
  value: string;
}

export interface ToolPresentation {
  viewKey: string;
  title: string;
  summary: string;
  status: ToolViewStatus;
  metrics?: ToolMetric[];
  details?: ToolDetail[];
}

export interface ToolViewInput {
  name: string;
  viewKey?: string;
  args: unknown;
  result?: string | null;
  evidence?: unknown[];
  status?: 'running' | 'ok' | 'failed';
  error?: string;
}

export interface ToolView {
  readonly key: string;
  present(input: ToolViewInput): ToolPresentation | null;
}
