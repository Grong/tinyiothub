import type {
  ApprovalState,
  DeviceJobState,
  EvidenceRef,
  RuntimeApproval,
  RuntimeDeviceAssociation,
  RuntimeDeviceFacts,
  RuntimeEvent,
  RuntimeJob,
  RuntimeTask,
  RuntimeWatch,
  TaskProjection,
  TaskStatus,
} from '../../types/runtime.js';
import {
  TERMINAL_TASK_STATUSES,
  isApprovalState,
  isJobState,
  isTaskStatus,
} from './runtime-status.js';

export type RuntimeDeviceRef = RuntimeDeviceAssociation &
  Partial<Omit<RuntimeDeviceFacts, 'deviceId'>>;

export type RuntimeProjectedJob = Partial<RuntimeJob> & Pick<RuntimeJob, 'id' | 'state'>;
export type RuntimeProjectedApproval = Partial<RuntimeApproval> &
  Pick<RuntimeApproval, 'id' | 'state'>;
export type RuntimeConnection = 'connecting' | 'online' | 'offline';

function byCursor(a: RuntimeEvent, b: RuntimeEvent): number {
  return a.cursor - b.cursor || a.eventId.localeCompare(b.eventId);
}

function watchId(event: RuntimeEvent): string | null {
  if (typeof event.data.watchId === 'string') return event.data.watchId;
  const match = event.eventId.match(/^watch:(.+):(created|completed|deleted)$/);
  return match?.[1] ?? null;
}

export function groupRuntimeTasks(tasks: RuntimeTask[]): {
  current: RuntimeTask[];
  recent: RuntimeTask[];
} {
  const byActivity = (a: RuntimeTask, b: RuntimeTask) =>
    b.updatedAt.localeCompare(a.updatedAt) || a.id.localeCompare(b.id);
  return {
    current: tasks.filter((task) => !TERMINAL_TASK_STATUSES.has(task.status)).sort(byActivity),
    recent: tasks.filter((task) => TERMINAL_TASK_STATUSES.has(task.status)).sort(byActivity),
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
  highWatermark = 0;
  incompleteTail = false;
  lastPersistedSeq: number | null = null;

  frozenHistory: boolean;
  eventIds = new Set<string>();
  private stepCursors = new Set<number>();
  private baseSnapshot: TaskProjection;
  private baseCursor = 0;

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
      approvals: this.approvals.filter((approval): approval is RuntimeApproval =>
        this.isCompleteApproval(approval),
      ),
      watches: [...this.watches],
      evidence: [...this.evidence],
      associations: this.devices.map(({ deviceId, sourceIds }) => ({
        deviceId,
        sourceIds: [...sourceIds],
      })),
      devices: this.devices.flatMap((device) => {
        if (typeof device.health !== 'string' || !device.sources || !device.fields) return [];
        return [
          {
            deviceId: device.deviceId,
            health: device.health,
            zone: device.zone ?? null,
            position: device.position ?? null,
            sources: device.sources.map((source) => ({ ...source })),
            fields: device.fields.map((field) => ({ ...field })),
          },
        ];
      }),
      cursor: this.cursor,
    };
  }

  get stale(): boolean {
    return this.connection !== 'online';
  }

  setConnection(connection: RuntimeConnection): void {
    this.connection = connection;
  }

  /** Keep already-rendered steps while a history snapshot freezes the projection. */
  restoreHistorySteps(events: RuntimeEvent[]): void {
    this.frozenHistory = true;
    this.steps = [...events].sort(byCursor);
    this.eventIds = new Set(this.steps.map((event) => event.eventId));
    this.stepCursors = new Set(this.steps.map((event) => event.cursor));
    this.scanInterruptedTail(this.steps);
  }

  private scanInterruptedTail(events: RuntimeEvent[]): void {
    this.incompleteTail = false;
    this.lastPersistedSeq = null;
    for (const event of events) {
      if (event.eventType !== 'run.interrupted') continue;
      const incomplete = event.data.incompleteTail ?? event.data.incomplete_tail;
      const lastPersisted = event.data.lastPersistedSeq ?? event.data.last_persisted_seq;
      if (incomplete === true) this.incompleteTail = true;
      if (typeof lastPersisted === 'number') this.lastPersistedSeq = lastPersisted;
    }
  }

  /** Merge a bounded timeline page or live deltas into the shown steps. */
  mergeSteps(events: RuntimeEvent[]): void {
    if (this.frozenHistory) return;
    let changed = false;
    for (const event of events) {
      if (this.eventIds.has(event.eventId) || this.stepCursors.has(event.cursor)) continue;
      this.eventIds.add(event.eventId);
      this.stepCursors.add(event.cursor);
      this.steps.push(event);
      changed = true;
    }
    if (!changed) return;
    this.steps.sort(byCursor);
    this.cursor = Math.max(this.cursor, this.steps.at(-1)?.cursor ?? this.cursor);
    this.rebuild();
  }

  apply(event: RuntimeEvent, highWatermark = event.cursor): void {
    if (this.frozenHistory) return;
    this.highWatermark = Math.max(this.highWatermark, highWatermark);
    this.mergeSteps([event]);
  }

  replaceWithSnapshot(snapshot: TaskProjection, cursor = snapshot.cursor, highWatermark = cursor): void {
    // Binding user decision: a same-Task snapshot must never erase the timeline.
    // Only a Task switch (or an explicit clear) replaces the shown steps.
    const sameTask = snapshot.task.id === this.baseSnapshot.task.id;
    // A stale same-Task response must not regress Task/job state that newer
    // events already advanced; ignoring it is safer than partially merging.
    if (sameTask && cursor < this.baseCursor) return;
    const retainedSteps = sameTask ? this.steps : [];
    this.baseSnapshot = {
      ...snapshot,
      task: { ...snapshot.task },
      runs: snapshot.runs.map((run) => ({ ...run })),
      jobs: snapshot.jobs.map((job) => ({ ...job })),
      approvals: snapshot.approvals.map((approval) => ({ ...approval })),
      watches: snapshot.watches.map((watch) => ({ ...watch })),
      evidence: snapshot.evidence.map((evidence) => ({ ...evidence })),
      associations: snapshot.associations.map((association) => ({
        ...association,
        sourceIds: [...association.sourceIds],
      })),
      devices: snapshot.devices.map((device) => ({
        ...device,
        sources: device.sources.map((source) => ({ ...source })),
        fields: device.fields.map((field) => ({ ...field })),
      })),
      cursor,
    };
    // Never move the snapshot guard backwards on a stale same-Task response.
    this.baseCursor = sameTask ? Math.max(this.baseCursor, cursor) : cursor;
    this.highWatermark = sameTask ? Math.max(this.highWatermark, highWatermark) : highWatermark;
    this.steps = [...retainedSteps].sort(byCursor);
    this.cursor = Math.max(cursor, this.steps.at(-1)?.cursor ?? cursor);
    this.eventIds = new Set(this.steps.map((event) => event.eventId));
    this.stepCursors = new Set(this.steps.map((event) => event.cursor));
    this.rebuild();
  }

  private rebuild(): void {
    const task = { ...this.baseSnapshot.task };
    this.jobs = this.baseSnapshot.jobs.map((job) => ({ ...job }));
    this.approvals = this.baseSnapshot.approvals.map((approval) => ({ ...approval }));
    this.watches = this.baseSnapshot.watches.map((watch) => ({ ...watch }));
    this.evidence = this.baseSnapshot.evidence.map((evidence) => ({ ...evidence }));
    this.devices = [];

    for (const association of this.baseSnapshot.associations) {
      this.associate(association.deviceId);
      for (const sourceId of association.sourceIds) this.associate(association.deviceId, sourceId);
      const facts = this.baseSnapshot.devices.find(
        (device) => device.deviceId === association.deviceId,
      );
      const device = this.devices.find((item) => item.deviceId === association.deviceId);
      if (device && facts)
        Object.assign(device, {
          health: facts.health,
          zone: facts.zone,
          position: facts.position,
          sources: facts.sources.map((source) => ({ ...source })),
          fields: facts.fields.map((field) => ({ ...field })),
        });
    }

    for (const event of this.steps) {
      const deviceId =
        event.deviceId ??
        (typeof event.data.deviceId === 'string' ? event.data.deviceId : null);
      if (
        deviceId !== null ||
        event.jobId ||
        event.approvalId ||
        event.eventType.startsWith('watch.') ||
        event.sourceId ||
        event.evidenceId
      ) {
        this.associate(deviceId, event.sourceId);
        if (deviceId && Array.isArray(event.data.sourceIds)) {
          for (const sourceId of event.data.sourceIds) {
            if (typeof sourceId === 'string') this.associate(deviceId, sourceId);
          }
        }
      }
      this.applyDeviceFacts(event.data.deviceFacts);
      if (event.cursor <= this.baseCursor) continue;

      this.applyTaskEvent(task, event);
      if (event.jobId) this.upsertJob(event);
      if (event.approvalId) this.upsertApproval(event);
      if (event.eventType.startsWith('watch.')) this.upsertWatch(event);
      if (event.evidenceId) this.upsertEvidence(event);
    }
    this.scanInterruptedTail(this.steps);
    this.baseSnapshot.task = task;
  }

  private applyTaskEvent(task: RuntimeTask, event: RuntimeEvent): void {
    if (event.eventType.startsWith('task.')) {
      const status = event.eventType.slice('task.'.length) as TaskStatus;
      const version =
        typeof event.data.stateVersion === 'number' ? event.data.stateVersion : task.stateVersion;
      if (isTaskStatus(status) && version >= task.stateVersion) {
        task.status = status;
        task.stateVersion = version;
        task.updatedAt = event.occurredAt;
        if (TERMINAL_TASK_STATUSES.has(status)) task.endedAt = event.occurredAt;
      }
    }
    if (
      event.runId &&
      (event.eventType === 'run.started' || event.eventType === 'task.run_bound')
    ) {
      task.currentRunId = event.runId;
    }
  }

  private upsertJob(event: RuntimeEvent): void {
    const id = event.jobId!;
    const state =
      typeof event.data.state === 'string' ? (event.data.state as DeviceJobState) : null;
    let job = this.jobs.find((item) => item.id === id);
    if (!job && state && isJobState(state)) {
      job = { id, state };
      this.jobs.push(job);
    }
    if (!job) return;
    if (state && isJobState(state)) job.state = state;
    job.workspaceId = event.workspaceId;
    if (event.deviceId) job.deviceId = event.deviceId;
    job.taskId = event.taskId;
    job.runId = event.runId;
    job.toolCallId = event.toolCallId;
    if (typeof event.data.tool === 'string') job.tool = event.data.tool;
    if ('argsJson' in event.data) job.argsJson = event.data.argsJson;
    if ('progressJson' in event.data) job.progressJson = event.data.progressJson;
    if ('resultJson' in event.data)
      job.resultJson = event.data.resultJson as RuntimeJob['resultJson'];
    if (typeof event.data.driverCallId === 'string' || event.data.driverCallId === null) {
      job.driverCallId = event.data.driverCallId as string | null;
    }
    if (typeof event.data.stopRequestedAt === 'string' || event.data.stopRequestedAt === null) {
      job.stopRequestedAt = event.data.stopRequestedAt as string | null;
    }
    if (typeof event.data.timeoutMs === 'number') job.timeoutMs = event.data.timeoutMs;
    if (typeof event.data.startedAt === 'string') job.startedAt = event.data.startedAt;
    if (typeof event.data.endedAt === 'string' || event.data.endedAt === null) {
      job.endedAt = event.data.endedAt as string | null;
    }
    if (isJobState(job.state) && !['accepted', 'running'].includes(job.state) && !job.endedAt) {
      job.endedAt = event.occurredAt;
    }
  }

  private upsertApproval(event: RuntimeEvent): void {
    const id = event.approvalId!;
    const state = typeof event.data.state === 'string' ? (event.data.state as ApprovalState) : null;
    let approval = this.approvals.find((item) => item.id === id);
    if (!approval && state && isApprovalState(state)) {
      approval = { id, state };
      this.approvals.push(approval);
    }
    if (!approval) return;
    if (state && isApprovalState(state)) approval.state = state;
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
      sourceSeq:
        event.sourceSeq ?? (typeof event.data.sourceSeq === 'number' ? event.data.sourceSeq : 0),
      mediaTime: typeof event.data.mediaTime === 'string' ? event.data.mediaTime : event.occurredAt,
      expiresAt: typeof event.data.expiresAt === 'string' ? event.data.expiresAt : null,
      deviceId: event.deviceId ?? '',
      sourceId: event.sourceId ?? '',
      taskId: event.taskId,
      runId: event.runId,
      toolCallId: event.toolCallId,
      jobId: event.jobId,
    });
  }

  private applyDeviceFacts(raw: unknown): void {
    if (!raw || typeof raw !== 'object') return;
    const facts = raw as Record<string, unknown>;
    if (typeof facts.deviceId !== 'string') return;
    this.associate(facts.deviceId);
    const device = this.devices.find((item) => item.deviceId === facts.deviceId);
    if (!device) return;
    if (typeof facts.health === 'string') device.health = facts.health;
    const zone = facts.zone;
    if (typeof zone === 'string' || zone === null) device.zone = zone;
    const position = facts.position;
    if (typeof position === 'string' || position === null) device.position = position;
    if (Array.isArray(facts.sources)) {
      device.sources = facts.sources.flatMap((source) => {
        if (!source || typeof source !== 'object') return [];
        const item = source as Record<string, unknown>;
        return typeof item.id === 'string' && typeof item.dataKind === 'string'
          ? [{ id: item.id, dataKind: item.dataKind }]
          : [];
      });
    }
    if (Array.isArray(facts.fields)) {
      device.fields = facts.fields.flatMap((field) => {
        if (!field || typeof field !== 'object') return [];
        const item = field as Record<string, unknown>;
        if (typeof item.name !== 'string') return [];
        return [
          {
            name: item.name,
            value: typeof item.value === 'string' ? item.value : null,
            unit: typeof item.unit === 'string' ? item.unit : null,
          },
        ];
      });
    }
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
    return (
      typeof job.workspaceId === 'string' &&
      typeof job.deviceId === 'string' &&
      typeof job.tool === 'string' &&
      typeof job.startedAt === 'string'
    );
  }

  private isCompleteApproval(approval: RuntimeProjectedApproval): approval is RuntimeApproval {
    return (
      typeof approval.workspaceId === 'string' &&
      typeof approval.taskId === 'string' &&
      typeof approval.runId === 'string' &&
      typeof approval.deviceId === 'string' &&
      typeof approval.tool === 'string'
    );
  }
}
