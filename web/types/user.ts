/**
 * 用户相关类型定义
 * 前端统一使用 camelCase 命名
 */

export interface User {
  id: string
  name: string
  phone?: string
  email?: string
  avatar?: string
  dateLastLogon?: string
  isDisabled: boolean
  parentId?: string
}

export interface UserProfile extends User {
  role?: string
  permissions?: string[]
  createdAt?: string
  updatedAt?: string
}

export interface LoginRequest {
  username: string
  password: string
}

export interface LoginResponse {
  accessToken: string
  tokenType: string
  expiresIn: number
  userInfo: User
}

export interface CreateUserRequest {
  name: string
  username: string
  password: string
  email?: string
  phone?: string
  role?: string
  parentId?: string
}

export interface UpdateUserRequest {
  name?: string
  email?: string
  phone?: string
  isDisabled?: boolean
}

export interface ChangePasswordRequest {
  oldPassword: string
  newPassword: string
}