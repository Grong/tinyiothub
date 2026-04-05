import { atom, computed } from 'nanostores'

export interface User {
  id: string
  name: string
  email?: string
  phone?: string
  avatar?: string
}

export const $token = atom<string | null>(
  typeof window !== 'undefined' ? sessionStorage.getItem('auth-token') : null
)
export const $user = atom<User | null>(null)

export const $isAuthenticated = computed([$token], (token) => !!token)

// Persist to sessionStorage
$token.subscribe((token) => {
  if (typeof window !== 'undefined') {
    if (token) {
      sessionStorage.setItem('auth-token', token)
    } else {
      sessionStorage.removeItem('auth-token')
    }
  }
})

// Actions
export function setAuth(token: string, user: User) {
  $token.set(token)
  $user.set(user)
}

export function clearAuth() {
  $token.set(null)
  $user.set(null)
}

// Listen for 401 errors from API client
if (typeof window !== 'undefined') {
  window.addEventListener('auth-error', () => {
    clearAuth()
    window.location.href = '/signin'
  })
}
