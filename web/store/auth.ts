import { createStore } from 'zustand/vanilla'
import { persist } from 'zustand/middleware'
import { immer } from 'zustand/middleware/immer'
import { apiPost, apiGet } from '@/lib/api-client'
import type { User, LoginResponse } from '@/types/user'

export interface AuthState {
  user: User | null
  token: string | null
  isAuthenticated: boolean
  isLoading: boolean
}

export interface AuthActions {
  login: (username: string, password: string) => Promise<void>
  logout: () => void
  setUser: (user: User) => void
  setToken: (token: string) => void
  initialize: () => void
  fetchUserProfile: () => Promise<void>
}

export type AuthStore = AuthState & AuthActions

const initialState: AuthState = {
  user: null,
  token: null,
  isAuthenticated: false,
  isLoading: false, // 改为 false，避免初始加载状态
}

export const createAuthStore = () =>
  createStore<AuthStore>()(
    persist(
      immer((set, get) => ({
        ...initialState,
        
        login: async (username: string, password: string) => {
          set((state) => {
            state.isLoading = true
          })

          try {
            const response = await apiPost<LoginResponse>('auth/login', {
              username,
              password,
            })

            if (response.code === 0 && response.result) {
              const { accessToken, userInfo } = response.result

              // 保存token到localStorage
              if (typeof window !== 'undefined') {
                localStorage.setItem('auth-token', accessToken)
                // 设置cookie用于SSR（可选）
                const isSecure = location.protocol === 'https:'
                const securePart = isSecure ? '; Secure' : ''
                document.cookie = `auth-token=${accessToken}; path=/; max-age=${24 * 60 * 60}; SameSite=Lax${securePart}`
              }

              set((state) => {
                state.user = userInfo
                state.token = accessToken
                state.isAuthenticated = true
                state.isLoading = false
              })
            } else {
              throw new Error(response.msg || 'Login failed')
            }
          } catch (error) {
            set((state) => {
              state.isLoading = false
            })
            throw error
          }
        },

        logout: () => {
          // 清除localStorage和cookie中的token
          if (typeof window !== 'undefined') {
            localStorage.removeItem('auth-token')
            // 清除cookie
            document.cookie = 'auth-token=; path=/; expires=Thu, 01 Jan 1970 00:00:00 GMT'
          }

          set((state) => {
            state.user = null
            state.token = null
            state.isAuthenticated = false
            state.isLoading = false
          })
        },

        setUser: (user: User) => {
          set((state) => {
            state.user = user
          })
        },

        setToken: (token: string) => {
          set((state) => {
            state.token = token
          })
        },

        fetchUserProfile: async () => {
          try {
            const response = await apiGet<User>('auth/session/profile')
            
            if (response.code === 0 && response.result) {
              set((state) => {
                state.user = response.result
              })
            }
          } catch (error) {
            console.error('Failed to fetch user profile:', error)
            // If it's an auth error, logout the user
            if (error instanceof Error && error.message.includes('Unauthorized')) {
              get().logout()
            }
          }
        },

        initialize: () => {
          console.log('Auth store initializing...')
          set((state) => {
            // 只在客户端执行初始化逻辑
            if (typeof window !== 'undefined') {
              // 从localStorage获取token
              const token = localStorage.getItem('auth-token')
              console.log('Token from localStorage:', token ? 'exists' : 'none')
              
              // 检查当前路径
              const isSigninPage = window.location.pathname.includes('/signin')
              const isRootPage = window.location.pathname === '/'
              
              if (token) {
                // 有token就设置认证状态为true
                state.token = token
                state.isAuthenticated = true
                console.log('Auth: token found, authenticated')
                
                // 如果有用户信息就直接使用，否则异步获取
                if (!state.user && !isSigninPage && !isRootPage) {
                  console.log('Auth: fetching user profile...')
                  // 异步获取用户信息，不阻塞初始化
                  setTimeout(() => {
                    get().fetchUserProfile().catch((error) => {
                      console.error('Failed to fetch user profile during initialization:', error)
                      // 如果获取用户信息失败，清除认证状态并跳转登录
                      get().logout()
                      window.location.href = '/signin'
                    })
                  }, 100)
                }
              } else {
                // 没有token，确保认证状态为false
                state.isAuthenticated = false
                console.log('Auth: no token, not authenticated')
              }

              // 监听全局认证错误事件
              const handleAuthError = () => {
                console.warn('Authentication error detected, logging out user')
                get().logout()
              }
              
              window.addEventListener('auth-error', handleAuthError)
            }
            state.isLoading = false
            console.log('Auth store initialized, isLoading:', false, 'isAuthenticated:', state.isAuthenticated)
          })
        },
      })),
      {
        name: 'auth-storage',
        partialize: (state) => ({
          user: state.user,
          token: state.token,
          isAuthenticated: state.isAuthenticated,
        }),
      }
    )
  )