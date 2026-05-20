/**
 * Memory API client — agent memory CRUD + reflection queue review.
 */
import { apiGet, apiPost, apiPut, type ApiResponse } from './client';

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
  agentId: string,
): Promise<ApiResponse<AgentMemory[]>> {
  return apiGet(`/workspaces/memories?agent_id=${agentId}`);
}

export async function getPendingQueue(
  agentId: string,
): Promise<ApiResponse<ReflectionQueueItem[]>> {
  return apiGet(`/workspaces/memories/queue?agent_id=${agentId}`);
}

export async function resolveQueueItem(
  queueId: string,
  approved: boolean,
  reviewerNote?: string,
): Promise<ApiResponse<{ resolved: boolean }>> {
  return apiPost(
    `/workspaces/memories/queue/${queueId}/resolve`,
    { approved, reviewer_note: reviewerNote },
  );
}

export async function pinMemory(
  memoryId: string,
  pinned: boolean,
): Promise<ApiResponse<{ pinned: boolean }>> {
  return apiPut(
    `/workspaces/memories/${memoryId}/pin`,
    { pinned },
  );
}
