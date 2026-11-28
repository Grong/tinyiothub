export type ToolViewStatus = 'running' | 'ok' | 'failed';

export interface ToolMetric {
  label: string;
  value: string;
}

export interface ToolDetail {
  label: string;
  value: string;
}

export interface ToolImage {
  evidenceId: string;
  alt: string;
  caption?: string;
}

export interface ToolPresentation {
  viewKey: string;
  title: string;
  summary: string;
  status: ToolViewStatus;
  /** 调用参数（筛选条件/时间范围等），用于在卡片上区分“看起来一样”的调用。 */
  params?: ToolDetail[];
  metrics?: ToolMetric[];
  details?: ToolDetail[];
  /** Evidence-backed images rendered by the tool card (chat) and vision deck. */
  images?: ToolImage[];
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

/** 业务语气：决定「工作纪实」里的措辞与颜色语义。 */
export type ToolTone = 'read' | 'write' | 'alarm' | 'plan' | 'ask' | 'analysis' | 'skill' | 'think';

/** 这张工具卡在「工作纪实」里的视觉族（视觉语言，不是工具清单）。 */
export type CardFamily =
  | 'skill'
  | 'device'
  | 'list'
  | 'doc'
  | 'vision'
  | 'think'
  | 'finding'
  | 'action'
  | 'plan'
  | 'subagent'
  | 'watch'
  | 'generic';

export interface ToolView {
  readonly key: string;
  /** 卡片视觉族；缺省按 viewKey 前缀推断，未知 viewKey 走 generic。 */
  readonly card?: CardFamily;
  /** 业务语气（读写/告警/计划…），供故事页使用。 */
  readonly tone?: ToolTone;
  /** 业务动词，用于没有专属标题时的兜底文案。 */
  readonly verb?: string;
  present(input: ToolViewInput): ToolPresentation | null;
}
