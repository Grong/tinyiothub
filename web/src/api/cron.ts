/**
 * 定时任务 API
 */

import { apiGet, apiPost, apiPut, apiDelete } from './client.js';
import type {
  Job,
  JobExecution,
  JobStatistics,
  JobQueryParams,
  CreateJobRequest,
  UpdateJobRequest,
} from '../types/index.js';
import type { PaginatedResponse } from './client.js';

export const cronApi = {
  async getJobs(params?: JobQueryParams) {
    return apiGet<Job[]>('/jobs', params);
  },

  async getJob(id: string) {
    return apiGet<Job>(`/jobs/${id}`);
  },

  async createJob(data: CreateJobRequest) {
    return apiPost<Job>('/jobs', data);
  },

  async updateJob(id: string, data: UpdateJobRequest) {
    return apiPut<Job>(`/jobs/${id}`, data);
  },

  async deleteJob(id: string) {
    return apiDelete<void>(`/jobs/${id}`);
  },

  async runJobNow(id: string) {
    return apiPost<JobExecution>(`/jobs/${id}/run`);
  },

  async getJobExecutions(id: string, limit?: number) {
    return apiGet<JobExecution[]>(`/jobs/${id}/executions`, limit ? { page_size: limit } : undefined);
  },

  async getStatistics() {
    return apiGet<JobStatistics>('/jobs/statistics');
  },

  async getAllExecutions(params?: { page?: number; pageSize?: number; jobId?: string; status?: string }) {
    return apiGet<PaginatedResponse<JobExecution>>('/jobs/executions', params);
  },
};
