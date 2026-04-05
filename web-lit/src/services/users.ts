/**
 * 用户管理服务 - Pure async API functions
 */

import { apiGet, apiPost, apiPut, apiDelete } from '../lib/api-client'

// Types
export interface User {
  id: string
  name: string
  username: string
  email: string
  phone?: string
  avatar?: string
  role: string
  isDisabled: boolean
  createdAt: string
  updatedAt: string
}

export interface CreateUserRequest {
  name: string
  username: string
  email: string
  phone?: string
  password: string
  role: string
}

export interface UpdateUserRequest {
  name?: string
  email?: string
  phone?: string
  avatar?: string
  role?: string
}

export interface ChangePasswordRequest {
  oldPassword: string
  newPassword: string
}

export interface UserStatistics {
  totalUsers: number
  activeUsers: number
  disabledUsers: number
}

// Pure async API functions
export const userApi = {
  getUsers: (params?: {
    enabled?: boolean
    search?: string
    page?: number
    page_size?: number
  }) => apiGet<User[]>('users', params),

  getUser: (id: string) => apiGet<User>(`users/${id}`),

  createUser: (data: CreateUserRequest) => apiPost<User>('users', data),

  updateUser: (id: string, data: UpdateUserRequest) => apiPut<User>(`users/${id}`, data),

  deleteUser: (id: string) => apiDelete<boolean>(`users/${id}`),

  enableUser: (id: string) => apiPost<boolean>(`users/${id}/enable`),

  disableUser: (id: string) => apiPost<boolean>(`users/${id}/disable`),

  changeUserPassword: (id: string, data: ChangePasswordRequest) =>
    apiPut<boolean>(`users/${id}/password`, data),

  getUserStatistics: () => apiGet<UserStatistics>('users/statistics'),
}

// Query keys for consistency with React Query usage
export const userKeys = {
  all: ['users'] as const,
  lists: () => [...userKeys.all, 'list'] as const,
  list: (filters: Record<string, any>) => [...userKeys.lists(), { filters }] as const,
  details: () => [...userKeys.all, 'detail'] as const,
  detail: (id: string) => [...userKeys.details(), id] as const,
  statistics: () => [...userKeys.all, 'statistics'] as const,
}
