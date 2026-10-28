import type {
  ApprovalState,
  DeviceJobState,
  EvidenceRef,
  RuntimeApproval,
  RuntimeEvent,
  RuntimeJob,
  RuntimeTask,
  RuntimeWatch,
  TaskProjection,
  TaskStatus,
} from '../../types/runtime.js';

const associatedTools = new Set(['read_device', 'call_device', 'set_device', 'stop_device', 'watch_device']);
const terminalTasks = new Set<TaskStatus>(['completed', 'failed', 'cancelled']);
const taskStatuses = new Set<TaskStatus>([
  'active', 'waiting_job', 'waiting_watch', 'needs_approval', 'paused', 'cancelling',
  'completed', 'failed', 'cancelled',
]);
const jobStates = new Set<DeviceJobState>([
  'accepted', 'running', 'done', 'failed', 'rejected', 'cancelled', 'cancelled_unknown', 'interrupted',
]);
const approvalStates = new Set<ApprovalState>(['pending', 'approved', 'rejected', 'expired', 'unavailable']);

export interface RuntimeDeviceRef {
  deviceId: string;
  sourceIds: string[];
}

export type RuntimeProjectedJob = Partial<RuntimeJob> & Pick<RuntimeJob, 'id' | 'state'>;
export type RuntimeProjectedApproval = Partial<RuntimeApproval> & Pick<RuntimeApproval, 'id' | 'state'>;
export type RuntimeConnection = 'connecting' | 'online' | 'offline';

function watchId(event: RuntimeEvent): string | null {
  if (typeof event.data.watchId === 'string') return event.data.watchId;
  const match = event.eventId.match(/^watch:(.+):(created|completed|deleted)$/);
  return match?.[1] ?? null;
}

function watchDevice(watch: RuntimeWatch): string | null {
  if (watch.condition === null || typeof watch.condition !== 'object' || Array.isArray(watch.condition)) {
    return null;
  }
  const condition = watch.condition as Record<string, unknown>;
  const deviceId = condition.thingId ?? condition.thing_id;
  return typeof deviceId === 'string' ? deviceId : null;
}

export function groupRuntimeTasks(tasks: RuntimeTask[]): { current: RuntimeTask[]; recent: RuntimeTask[] } {
  const byActivity = (a: RuntimeTask, b: RuntimeTask) =>
    b.updatedAt.localeCompare(a.updatedAt) || a.id.localeCompare(b.id);
  return {
    current: tasks.filter((task) => !terminalTasks.has(task.status)).sort(byActivity),
    recent: tasks.filter((task) => terminalTasks.has(task.status)).sort(byActivity),
  };
}

export class RuntimeProjection {
  steps: RuntimeEvent[] = [];
  devices: RuntimeDeviceRef[] = [];
  jobs: RuntimeProjectedJob[] = [];
  approvals: RuntimeProjectedApproval[] = [];
  watches: RuntimeWatch[] = [];
  evidence: EvidenceRef[] = [];
  connection: RuntimeConnection = 'connecting';
  cursor = 0;

  readonly frozenHistory: boolean;
  private baseSnapshot: TaskProjection;
  private baseCursor = 0;
  private eventIds = new Set<string>();

  constructor(snapshot: TaskProjection, options: { history?: boolean } = {}) {
    this.baseSnapshot = snapshot;
    this.frozenHistory = options.history ?? false;
    this.replaceWithSnapshot(snapshot);
  }

  get task(): RuntimeTask {
    return this.baseSnapshot.task;
  }

  get snapshot(): TaskProjection {
    return {
      ...this.baseSnapshot,
      jobs: this.jobs.filter((job): job is RuntimeJob => this.isCompleteJob(job)),
      approvals: this.approvals.filter((approval): approval is RuntimeApproval => this.isCompleteApproval(approval)),
      watches: [...this.watches],
      evidence: [...this.evidence],
      cursor: this.cursor,
    };
  }

  get stale(): boolean {
    return this.connection !== 'online';
  }

  setConnection(connection: RuntimeConnection): void {
    this.connection = connection;
  }

  apply(event: RuntimeEvent): void {
    if (this.eventIds.has(event.eventId) || this.frozenHistory) return;
    this.eventIds.add(event.eventId);
    this.steps.push(event);
    this.steps.sort((a, b) => a.cursor - b.cursor || a.eventId.localeCompare(b.eventId));
    this.cursor = Math.max(this.cursor, event.cursor);
    this.rebuild();
  }

  replaceWithSnapshot(snapshot: TaskProjection, cursor = snapshot.cursor): void {
    this.baseSnapshot = {
      ...snapshot,
      task: { ...snapshot.task },
      runs: snapshot.runs.map((run) => ({ ...run })),
      jobs: snapshot.jobs.map((job) => ({ ...job })),
      approvals: snapshot.approvals.map((approval) => ({ ...approval })),
      watches: snapshot.watches.map((watch) => ({ ...watch })),
      evidence: snapshot.evidence.map((evidence) => ({ ...evidence })),
      cursor,
    };
    this.baseCursor = cursor;
    this.cursor = cursor;
    this.eventIds.clear();
    this.steps = [];
    this.rebuild();
  }

  private rebuild(): void {
    const task = { ...this.baseSnapshot.task };
    this.jobs = this.baseSnapshot.jobs.map((job) => ({ ...job }));
    this.approvals = this.baseSnapshot.approvals.map((approval) => ({ ...approval }));
    this.watches = this.baseSnapshot.watches.map((watch) => ({ ...watch }));
    this.evidence = this.baseSnapshot.evidence.map((evidence) => ({ ...evidence }));
    this.devices = [];

    for (const job of this.baseSnapshot.jobs) this.associate(job.deviceId);
    for (const approval of this.baseSnapshot.approvals) this.associate(approval.deviceId);
    for (const watch of this.baseSnapshot.watches) this.associate(watchDevice(watch));

    for (const event of this.steps) {
      const deviceId = event.deviceId ??
        (associatedTools.has(event.eventType) && typeof event.data.deviceId === 'string'
          ? event.data.deviceId
          : null);
      if (
        associatedTools.has(event.eventType) || event.jobId || event.approvalId ||
        event.eventType.startsWith('watch.') || event.sourceId || event.evidenceId
      ) {
        this.associate(deviceId, event.sourceId);
      }
      if (event.cursor <= this.baseCursor) continue;

      this.applyTaskEvent(task, event);
      if (event.jobId) this.upsertJob(event);
      if (event.approvalId) this.upsertApproval(event);
      if (event.eventType.startsWith('watch.')) this.upsertWatch(event);
      if (event.evidenceId) this.upsertEvidence(event);
    }
    this.baseSnapshot.task = task;
  }

  private applyTaskEvent(task: RuntimeTask, event: RuntimeEvent): void {
    if (event.eventType.startsWith('task.')) {
      const status = event.eventType.slice('task.'.length) as TaskStatus;
      const version = typeof event.data.stateVersion === 'number' ? event.data.stateVersion : task.stateVersion;
      if (taskStatuses.has(status) && version >= task.stateVersion) {
        task.status = status;
        task.stateVersion = version;
        task.updatedAt = event.occurredAt;
        if (terminalTasks.has(status)) task.endedAt = event.occurredAt;
      }
    }
    if (event.runId && (event.eventType === 'run.started' || event.eventType === 'task.run_bound')) {
      task.currentRunId = event.runId;
    }
  }

  private upsertJob(event: RuntimeEvent): void {
    const id = event.jobId!;
    const state = typeof event.data.state === 'string' ? event.data.state as DeviceJobState : null;
    let job = this.jobs.find((item) => item.id === id);
    if (!job && state && jobStates.has(state)) {
      job = { id, state };
      this.jobs.push(job);
    }
    if (!job) return;
    if (state && jobStates.has(state)) job.state = state;
    if (event.deviceId) job.deviceId = event.deviceId;
    job.taskId = event.taskId;
    job.runId = event.runId;
    job.toolCallId = event.toolCallId;
    if (jobStates.has(job.state) && !['accepted', 'running'].includes(job.state)) {
      job.endedAt = event.occurredAt;
    }
  }

  private upsertApproval(event: RuntimeEvent): void {
    const id = event.approvalId!;
    const state = typeof event.data.state === 'string' ? event.data.state as ApprovalState : null;
    let approval = this.approvals.find((item) => item.id === id);
    if (!approval && state && approvalStates.has(state)) {
      approval = { id, state };
      this.approvals.push(approval);
    }
    if (!approval) return;
    if (state && approvalStates.has(state)) approval.state = state;
    if (event.deviceId) approval.deviceId = event.deviceId;
    if (event.taskId) approval.taskId = event.taskId;
    if (event.runId) approval.runId = event.runId;
    if (event.toolCallId) approval.toolCallId = event.toolCallId;
    if (typeof event.data.decisionEventId === 'string') {
      approval.decisionEventId = event.data.decisionEventId;
    }
    if (state && state !== 'pending') approval.decidedAt = event.occurredAt;
    approval.updatedAt = event.occurredAt;
  }

  private upsertWatch(event: RuntimeEvent): void {
    const id = watchId(event);
    if (!id) return;
    if (event.eventType === 'watch.completed' || event.eventType === 'watch.deleted') {
      this.watches = this.watches.filter((watch) => watch.id !== id);
      return;
    }
    if (event.eventType === 'watch.created' && !this.watches.some((watch) => watch.id === id)) {
      this.watches.push({
        id,
        condition: event.deviceId ? { thing_id: event.deviceId } : null,
        persistent: false,
        timeoutSecs: 0,
        createdAt: event.occurredAt,
      });
    }
  }

  private upsertEvidence(event: RuntimeEvent): void {
    if (this.evidence.some((item) => item.evidenceId === event.evidenceId)) return;
    if (typeof event.data.mime !== 'string') return;
    this.evidence.push({
      evidenceId: event.evidenceId!,
      mime: event.data.mime,
      sourceSeq: event.sourceSeq ?? 0,
      mediaTime: typeof event.data.mediaTime === 'string' ? event.data.mediaTime : event.occurredAt,
    });
  }

  private associate(deviceId: string | null, sourceId?: string | null): void {
    if (!deviceId) return;
    let device = this.devices.find((item) => item.deviceId === deviceId);
    if (!device) {
      device = { deviceId, sourceIds: [] };
      this.devices.push(device);
    }
    if (sourceId && !device.sourceIds.includes(sourceId)) device.sourceIds.push(sourceId);
  }

  private isCompleteJob(job: RuntimeProjectedJob): job is RuntimeJob {
    return typeof job.workspaceId === 'string' && typeof job.deviceId === 'string' &&
      typeof job.tool === 'string' && typeof job.startedAt === 'string';
  }

  private isCompleteApproval(approval: RuntimeProjectedApproval): approval is RuntimeApproval {
    return typeof approval.workspaceId === 'string' && typeof approval.taskId === 'string' &&
      typeof approval.runId === 'string' && typeof approval.deviceId === 'string' &&
      typeof approval.tool === 'string';
  }
}
