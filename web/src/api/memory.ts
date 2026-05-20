/**
 * Memory API client — agent memory CRUD + reflection queue review.
 */
import { apiGet, apiPost, type ApiResponse } from './client';

export interface AgentMemory {
  id: string;
  workspaceId: string;
  agentId: string;
  zone: 'core' | 'work' | 'episode' | 'general';
  content: string;
  source: 'user' | 'reflection' | 'import' | 'system' | 'deviceSnapshot';
  confidence: 'high' | 'medium' | 'low';
  tags: string[];
  pinned: boolean;
  supersedes: string | null;
  effectiveness: number;
  loadCount: number;
  referenceCount: number;
  createdAt: string;
  updatedAt: string;
}

export interface ReflectionQueueItem {
  id: string;
  candidateType: 'memory' | 'skill';
  candidateData: string;
  status: 'pending' | 'approved' | 'rejected';
  createdAt: string;
}

export async function listActiveMemories(
  workspaceId: string,
  agentId: string,
): Promise<ApiResponse<AgentMemory[]>> {
  return apiGet(`/workspaces/${workspaceId}/memories?agent_id=${agentId}`);
}

export async function getPendingQueue(
  workspaceId: string,
  agentId: string,
): Promise<ApiResponse<ReflectionQueueItem[]>> {
  return apiGet(`/workspaces/${workspaceId}/memories/queue?agent_id=${agentId}`);
}

export async function resolveQueueItem(
  workspaceId: string,
  queueId: string,
  approved: boolean,
  reviewerNote?: string,
): Promise<ApiResponse<{ resolved: boolean }>> {
  return apiPost(
    `/workspaces/${workspaceId}/memories/queue/${queueId}`,
    { approved, reviewer_note: reviewerNote },
  );
}

export async function pinMemory(
  workspaceId: string,
  memoryId: string,
  pinned: boolean,
): Promise<ApiResponse<{ pinned: boolean }>> {
  return apiPost(
    `/workspaces/${workspaceId}/memories/${memoryId}/pin`,
    { pinned },
  );
}
