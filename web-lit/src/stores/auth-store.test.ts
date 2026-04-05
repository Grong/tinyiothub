import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest'
import { $token, $user, $isAuthenticated, setAuth, clearAuth } from './auth-store'

// Mock sessionStorage
const mockSessionStorage = {
  data: {} as Record<string, string>,
  getItem: vi.fn((key: string) => mockSessionStorage.data[key] ?? null),
  setItem: vi.fn((key: string, value: string) => { mockSessionStorage.data[key] = value }),
  removeItem: vi.fn((key: string) => { delete mockSessionStorage.data[key] }),
}

vi.stubGlobal('sessionStorage', mockSessionStorage)
vi.stubGlobal('window', {
  location: { href: '' },
  addEventListener: vi.fn(),
  dispatchEvent: vi.fn(),
})

describe('auth-store', () => {
  beforeEach(() => {
    vi.clearAllMocks()
    mockSessionStorage.data = {}
    // Reset stores to initial state
    $token.set(null)
    $user.set(null)
  })

  describe('$token', () => {
    it('starts with null', () => {
      expect($token.get()).toBeNull()
    })

    it('can be set and retrieved', () => {
      $token.set('test-token-abc123')
      expect($token.get()).toBe('test-token-abc123')
    })

    it('persists to sessionStorage when set', () => {
      $token.set('my-token')
      expect(mockSessionStorage.setItem).toHaveBeenCalledWith('auth-token', 'my-token')
    })

    it('removes from sessionStorage when set to null', () => {
      $token.set('some-token')
      $token.set(null)
      expect(mockSessionStorage.removeItem).toHaveBeenCalledWith('auth-token')
    })
  })

  describe('$user', () => {
    it('starts with null', () => {
      expect($user.get()).toBeNull()
    })

    it('can store user object', () => {
      const user = { id: 'user-1', name: 'Test User', email: 'test@example.com' }
      $user.set(user)
      expect($user.get()).toEqual(user)
    })
  })

  describe('$isAuthenticated', () => {
    it('is false when token is null', () => {
      $token.set(null)
      expect($isAuthenticated.get()).toBe(false)
    })

    it('is true when token exists', () => {
      $token.set('valid-token')
      expect($isAuthenticated.get()).toBe(true)
    })

    it('is false for empty string token', () => {
      $token.set('')
      expect($isAuthenticated.get()).toBe(false)
    })
  })

  describe('setAuth', () => {
    it('sets both token and user', () => {
      const user = { id: 'user-1', name: 'Test User' }
      setAuth('jwt-token-xyz', user)

      expect($token.get()).toBe('jwt-token-xyz')
      expect($user.get()).toEqual(user)
    })
  })

  describe('clearAuth', () => {
    it('clears both token and user', () => {
      setAuth('some-token', { id: '1', name: 'User' })
      clearAuth()

      expect($token.get()).toBeNull()
      expect($user.get()).toBeNull()
    })
  })
})
