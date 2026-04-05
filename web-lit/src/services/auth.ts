/**
 * 认证服务 - Pure async API functions
 */

import { apiGet, apiPost } from '../lib/api-client'

// Types
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

// Pure async API functions
export const authApi = {
  login: (data: LoginRequest) =>
    apiPost<LoginResponse>('auth/login', data),

  logout: () =>
    apiPost<boolean>('auth/logout'),

  getProfile: () =>
    apiGet<UserProfile>('auth/session/profile'),

  updateProfile: (data: Partial<UserProfile>) =>
    apiPost<UserProfile>('auth/session/profile', data),

  changePassword: (data: ChangePasswordRequest) =>
    apiPost<boolean>('auth/session/password', data),

  refreshToken: () =>
    apiPost<{ accessToken: string; expiresIn: number }>('auth/refresh'),
}
