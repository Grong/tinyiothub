// web-lit/src/ui/controllers/auth.ts
import type { AppViewState } from '../app-view-state'
import type { User, LoginResponse } from '../types'
import { apiPost } from '../api-client'
import { navigate } from '../../lib/navigate'

export async function login(host: AppViewState, username: string, password: string): Promise<void> {
  host.authLoading = true
  try {
    const res = await apiPost<LoginResponse>('auth/login', { username, password }, {}, true)
    if (res.result) {
      host.token = res.result.accessToken
      host.user = res.result.userInfo
      host.connected = true
      sessionStorage.setItem('auth-token', res.result.accessToken)
      navigate('/dashboard')
    }
  } finally {
    host.authLoading = false
  }
}

export async function logout(host: AppViewState): Promise<void> {
  try {
    await apiPost('auth/logout')
  } catch {
    // Logout best-effort
  }
  host.token = null
  host.user = null
  host.connected = false
  sessionStorage.removeItem('auth-token')
  navigate('/signin')
}

export async function loadProfile(host: AppViewState): Promise<void> {
  const res = await apiPost<User>('auth/session/profile')
  if (res.result) {
    host.user = res.result
  }
}

export async function updateProfile(host: AppViewState, data: Partial<User>): Promise<void> {
  const res = await apiPost<User>('auth/session/profile', data)
  if (res.result) {
    host.user = res.result
  }
}

export async function changePassword(host: AppViewState, oldPassword: string, newPassword: string): Promise<void> {
  await apiPost('auth/session/password', { oldPassword, newPassword })
}
