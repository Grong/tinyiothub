/**
 * 用户管理服务
 * 使用 TanStack Query 进行数据获取和状态管理
 */

import { useQuery, useMutation, useQueryClient } from '@tanstack/react-query'
import { apiGet, apiPost, apiPut, apiDelete } from '@/lib/api-client'
import type { 
  User, 
  CreateUserRequest, 
  UpdateUserRequest, 
  ChangePasswordRequest 
} from '@/types'

export interface UserStatistics {
  totalUsers: number
  activeUsers: number
  disabledUsers: number
}

// Query Keys
export const userKeys = {
  all: ['users'] as const,
  lists: () => [...userKeys.all, 'list'] as const,
  list: (filters: Record<string, any>) => [...userKeys.lists(), { filters }] as const,
  details: () => [...userKeys.all, 'detail'] as const,
  detail: (id: string) => [...userKeys.details(), id] as const,
  statistics: () => [...userKeys.all, 'statistics'] as const,
}

// API 调用函数
export const userApi = {
  // 获取用户列表
  getUsers: (params?: {
    enabled?: boolean
    search?: string
    page?: number
    page_size?: number  // 使用 snake_case 匹配后端
  }) => apiGet<User[]>('users', params),

  // 获取用户详情
  getUser: (id: string) => apiGet<User>(`users/${id}`),

  // 创建用户
  createUser: (data: CreateUserRequest) => apiPost<User>('users', data),

  // 更新用户
  updateUser: (id: string, data: UpdateUserRequest) => apiPut<User>(`users/${id}`, data),

  // 删除用户
  deleteUser: (id: string) => apiDelete<boolean>(`users/${id}`),

  // 启用用户
  enableUser: (id: string) => apiPost<boolean>(`users/${id}/enable`),

  // 禁用用户
  disableUser: (id: string) => apiPost<boolean>(`users/${id}/disable`),

  // 修改用户密码
  changeUserPassword: (id: string, data: ChangePasswordRequest) => 
    apiPut<boolean>(`users/${id}/password`, data),

  // 获取用户统计
  getUserStatistics: () => apiGet<UserStatistics>('users/statistics'),
}

// React Query Hooks

/**
 * 获取用户列表
 */
export const useUsers = (params?: {
  enabled?: boolean
  search?: string
  page?: number
  page_size?: number  // 使用 snake_case 匹配后端
}) => {
  return useQuery({
    queryKey: userKeys.list(params || {}),
    queryFn: () => userApi.getUsers(params),
    staleTime: 1000 * 60 * 5, // 5分钟
  })
}

/**
 * 获取用户详情
 */
export const useUser = (id: string, enabled = true) => {
  return useQuery({
    queryKey: userKeys.detail(id),
    queryFn: () => userApi.getUser(id),
    enabled: enabled && !!id,
    staleTime: 1000 * 60 * 5, // 5分钟
  })
}

/**
 * 获取用户统计
 */
export const useUserStatistics = () => {
  return useQuery({
    queryKey: userKeys.statistics(),
    queryFn: userApi.getUserStatistics,
    staleTime: 1000 * 60 * 10, // 10分钟
  })
}

/**
 * 创建用户
 */
export const useCreateUser = () => {
  const queryClient = useQueryClient()

  return useMutation({
    mutationFn: userApi.createUser,
    onSuccess: () => {
      // 刷新用户列表
      queryClient.invalidateQueries({ queryKey: userKeys.lists() })
      queryClient.invalidateQueries({ queryKey: userKeys.statistics() })
    },
  })
}

/**
 * 更新用户
 */
export const useUpdateUser = () => {
  const queryClient = useQueryClient()

  return useMutation({
    mutationFn: ({ id, data }: { id: string; data: UpdateUserRequest }) =>
      userApi.updateUser(id, data),
    onSuccess: (_, { id }) => {
      // 刷新相关查询
      queryClient.invalidateQueries({ queryKey: userKeys.detail(id) })
      queryClient.invalidateQueries({ queryKey: userKeys.lists() })
    },
  })
}

/**
 * 删除用户
 */
export const useDeleteUser = () => {
  const queryClient = useQueryClient()

  return useMutation({
    mutationFn: userApi.deleteUser,
    onSuccess: () => {
      // 刷新用户列表
      queryClient.invalidateQueries({ queryKey: userKeys.lists() })
      queryClient.invalidateQueries({ queryKey: userKeys.statistics() })
    },
  })
}

/**
 * 启用/禁用用户
 */
export const useToggleUserStatus = () => {
  const queryClient = useQueryClient()

  return useMutation({
    mutationFn: ({ id, enabled }: { id: string; enabled: boolean }) =>
      enabled ? userApi.enableUser(id) : userApi.disableUser(id),
    onSuccess: (_, { id }) => {
      // 刷新相关查询
      queryClient.invalidateQueries({ queryKey: userKeys.detail(id) })
      queryClient.invalidateQueries({ queryKey: userKeys.lists() })
      queryClient.invalidateQueries({ queryKey: userKeys.statistics() })
    },
  })
}

/**
 * 修改用户密码
 */
export const useChangeUserPassword = () => {
  return useMutation({
    mutationFn: ({ id, data }: { id: string; data: ChangePasswordRequest }) =>
      userApi.changeUserPassword(id, data),
  })
}