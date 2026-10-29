export type TaskStatus =
  | 'active'
  | 'waiting_job'
  | 'waiting_watch'
  | 'needs_approval'
  | 'paused'
  | 'cancelling'
  | 'completed'
  | 'failed'
  | 'cancelled';

export type DeviceJobState =
  | 'accepted'
  | 'running'
  | 'done'
  | 'failed'
  | 'rejected'
  | 'cancelled'
  | 'cancelled_unknown'
  | 'interrupted';

export type ApprovalState = 'pending' | 'approved' | 'rejected' | 'expired' | 'unavailable';
export type ApprovalDecision = 'approve' | 'reject';

export interface RuntimeTask {
  id: string;
  workspaceId: string;
  idempotencyKey: string;
  objective: string;
  sessionKey: string | null;
  createdBy: string;
  status: TaskStatus;
  currentRunId: string | null;
  stateVersion: number;
  createdAt: string;
  updatedAt: string;
  endedAt: string | null;
}

export interface RuntimeJob {
  id: string;
  workspaceId: string;
  deviceId: string;
  tool: string;
  argsJson: unknown;
  state: DeviceJobState;
  stopRequestedAt: string | null;
  progressJson: unknown | null;
  resultJson: unknown | null;
  taskId: string | null;
  runId: string | null;
  toolCallId: string | null;
  driverCallId: string | null;
  timeoutMs: number;
  startedAt: string;
  endedAt: string | null;
}

export interface RuntimeApproval {
  id: string;
  workspaceId: string;
  taskId: string;
  runId: string;
  toolCallId: string;
  deviceId: string;
  tool: string;
  argsJson: unknown;
  reason: string;
  state: ApprovalState;
  decision: ApprovalDecision | null;
  deciderId: string | null;
  requestedAt: string;
  decidedAt: string | null;
  expiresAt: string;
  decisionEventId: string;
  updatedAt: string;
}

export interface RuntimeWatch {
  id: string;
  condition: unknown;
  persistent: boolean;
  timeoutSecs: number;
  createdAt: string;
}

export interface RuntimeRun {
  id: string;
  status: string;
  outcome: string;
  summary: string;
  createdAt: string;
}

export interface EvidenceRef {
  evidenceId: string;
  mime: string;
  sourceSeq: number;
  mediaTime: string;
  expiresAt: string | null;
  deviceId: string;
  sourceId: string;
  taskId: string | null;
  runId: string | null;
  toolCallId: string | null;
  jobId: string | null;
}

export interface RuntimeDeviceAssociation {
  deviceId: string;
  sourceIds: string[];
}

export interface RuntimeDeviceSource {
  id: string;
  dataKind: 'video' | 'image' | 'values' | string;
}

export interface RuntimeDeviceField {
  name: string;
  value: string | null;
  unit: string | null;
}

export interface RuntimeDeviceFacts {
  deviceId: string;
  health: 'ok' | 'attention' | 'bad' | 'unknown' | string;
  zone: string | null;
  position: string | null;
  sources: RuntimeDeviceSource[];
  fields: RuntimeDeviceField[];
}

export interface RuntimeEvent {
  cursor: number;
  eventId: string;
  eventType: string;
  occurredAt: string;
  workspaceId: string;
  taskId: string | null;
  runId: string | null;
  toolCallId: string | null;
  jobId: string | null;
  approvalId: string | null;
  deviceId: string | null;
  sourceId: string | null;
  sourceSeq: number | null;
  evidenceId: string | null;
  data: Record<string, unknown>;
}

export interface RuntimeEventsPage {
  events: RuntimeEvent[];
  oldestCursor: number | null;
  hasMore: boolean;
}

export interface TaskProjection {
  task: RuntimeTask;
  runs: RuntimeRun[];
  jobs: RuntimeJob[];
  approvals: RuntimeApproval[];
  watches: RuntimeWatch[];
  evidence: EvidenceRef[];
  associations: RuntimeDeviceAssociation[];
  devices: RuntimeDeviceFacts[];
  summary: string | null;
  cursor: number;
}

export interface CreateTaskResult {
  taskId: string;
  status: 'accepted';
  deduplicated: boolean;
}

export interface RuntimeTaskListResult {
  tasks: RuntimeTask[];
  limit: number;
  offset: number;
}

export interface RuntimeApprovalDecisionResult {
  approval: RuntimeApproval;
  deduplicated: boolean;
}

export type PlaybackProtocol = 'mjpeg' | 'hls';

export interface PlaybackDescriptor {
  protocol: PlaybackProtocol;
  url: string;
  expiresAt: string;
}
