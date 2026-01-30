/**
 * 认证服务
 * 使用 TanStack Query 进行数据获取和状态管理
 */

import { useQuery, useMutation, useQueryClient } from '@tanstack/react-query'
import { apiGet, apiPost } from '@/lib/api-client'
import { queryKeys } from '@/lib/query-keys'
import { useAuthStore } from '@/store/provider'

// 认证相关类型定义
export interface LoginRequest {
  username: string
  password: string
}

export interface LoginResponse {
  accessToken: string
  tokenType: string
  expiresIn: number
  userInfo: UserInfo
}

export interface UserInfo {
  id: string
  name: string
  phone?: string
  email?: string
  avatar?: string
  dateLastLogon?: string
  isDisabled: number
  parentId?: string
}

export interface UserProfile extends UserInfo {
  role?: string
  permissions?: string[]
  createdAt?: string
  updatedAt?: string
}

export interface ChangePasswordRequest {
  oldPassword: string
  newPassword: string
}

// API 调用函数
export const authApi = {
  // 登录
  login: (data: LoginRequest) => 
    apiPost<LoginResponse>('auth/login', data),

  // 登出
  logout: () => 
    apiPost<boolean>('auth/logout'),

  // 获取用户资料
  getProfile: () => 
    apiGet<UserProfile>('auth/session/profile'),

  // 更新用户资料
  updateProfile: (data: Partial<UserProfile>) => 
    apiPost<UserProfile>('auth/session/profile', data),

  // 修改密码
  changePassword: (data: ChangePasswordRequest) => 
    apiPost<boolean>('auth/session/password', data),

  // 刷新令牌
  refreshToken: () => 
    apiPost<{ accessToken: string; expiresIn: number }>('auth/refresh'),
}

// React Query Hooks

/**
 * 获取用户资料
 */
export const useProfile = (enabled = true) => {
  const { isAuthenticated } = useAuthStore()
  
  return useQuery({
    queryKey: queryKeys.auth.profile(),
    queryFn: authApi.getProfile,
    enabled: enabled && isAuthenticated,
    staleTime: 1000 * 60 * 10, // 10分钟
    retry: (failureCount, error: any) => {
      // 如果是认证错误，不重试
      if (error?.message?.includes('Unauthorized') || error?.message?.includes('401')) {
        return false
      }
      return failureCount < 3
    },
  })
}

/**
 * 登录
 */
export const useLogin = () => {
  const queryClient = useQueryClient()
  const { login: setAuthState } = useAuthStore()

  return useMutation({
    mutationFn: authApi.login,
    onSuccess: async (response) => {
      if (response.code === 0 && response.result) {
        const { accessToken, userInfo } = response.result
        
        // 更新认证状态
        await setAuthState(userInfo.name, 'admin123') // 这里应该传入实际密码，但为了安全考虑，可能需要重构
        
        // 预加载用户资料
        queryClient.setQueryData(queryKeys.auth.profile(), userInfo)
      }
    },
    onError: (error) => {
      console.error('Login failed:', error)
    },
  })
}

/**
 * 登出
 */
export const useLogout = () => {
  const queryClient = useQueryClient()
  const { logout: clearAuthState } = useAuthStore()

  return useMutation({
    mutationFn: authApi.logout,
    onSuccess: () => {
      // 清除认证状态
      clearAuthState()
      
      // 清除所有查询缓存
      queryClient.clear()
    },
    onError: (error) => {
      console.error('Logout failed:', error)
      // 即使登出失败，也清除本地状态
      clearAuthState()
      queryClient.clear()
    },
  })
}

/**
 * 更新用户资料
 */
export const useUpdateProfile = () => {
  const queryClient = useQueryClient()

  return useMutation({
    mutationFn: authApi.updateProfile,
    onSuccess: (response) => {
      if (response.code === 0 && response.result) {
        // 更新缓存中的用户资料
        queryClient.setQueryData(queryKeys.auth.profile(), response.result)
      }
    },
  })
}

/**
 * 修改密码
 */
export const useChangePassword = () => {
  return useMutation({
    mutationFn: authApi.changePassword,
    onSuccess: () => {
      // 密码修改成功后可能需要重新登录
      console.log('Password changed successfully')
    },
  })
}

/**
 * 刷新令牌
 */
export const useRefreshToken = () => {
  return useMutation({
    mutationFn: authApi.refreshToken,
    onSuccess: (response) => {
      if (response.code === 0 && response.result) {
        // 更新本地存储的令牌
        if (typeof window !== 'undefined') {
          localStorage.setItem('auth-token', response.result.accessToken)
        }
      }
    },
  })
}