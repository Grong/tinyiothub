/**
 * 用户管理 API
 */

import { apiGet, apiPost, apiPut, apiDelete } from './client.js';
import type {
  User,
  CreateUserRequest,
  UpdateUserRequest,
  ChangePasswordRequest,
} from '../types/index.js';
import type { PaginatedResponse } from './client.js';

export const userApi = {
  async getUsers(params?: { page?: number; pageSize?: number }) {
    return apiGet<PaginatedResponse<User>>('/users', params as Record<string, any>);
  },

  async getUser(id: string) {
    return apiGet<User>(`/users/${id}`);
  },

  async createUser(data: CreateUserRequest) {
    return apiPost<User>('/users', data);
  },

  async updateUser(id: string, data: UpdateUserRequest) {
    return apiPut<User>(`/users/${id}`, data);
  },

  async deleteUser(id: string) {
    return apiDelete<void>(`/users/${id}`);
  },

  async changePassword(id: string, data: ChangePasswordRequest) {
    return apiPut<void>(`/users/${id}/password`, data);
  },
};
