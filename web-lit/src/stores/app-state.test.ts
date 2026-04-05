import { describe, it, expect, vi, beforeEach } from 'vitest'
import {
  $currentRoute,
  $navCollapsed,
  $searchQuery,
  $alarmCount,
  $notifications,
  PUBLIC_ROUTES,
  isPublicRoute,
  setCurrentRoute,
  toggleNav,
  setSearchQuery,
  setAlarmCount,
  addNotification,
  markNotificationRead,
  clearNotifications,
} from './app-state'

// Mock crypto.randomUUID
vi.stubGlobal('crypto', {
  randomUUID: vi.fn(() => 'test-uuid-123'),
})

describe('app-state', () => {
  beforeEach(() => {
    vi.clearAllMocks()
    $currentRoute.set('home')
    $navCollapsed.set(false)
    $searchQuery.set('')
    $alarmCount.set(0)
    $notifications.set([])
  })

  describe('isPublicRoute', () => {
    it('returns true for public routes', () => {
      expect(isPublicRoute('home')).toBe(true)
      expect(isPublicRoute('signin')).toBe(true)
      expect(isPublicRoute('register')).toBe(true)
      expect(isPublicRoute('tenant/register')).toBe(true)
      expect(isPublicRoute('marketplace')).toBe(true)
      expect(isPublicRoute('installed-marketplace')).toBe(true)
    })

    it('returns false for private routes', () => {
      expect(isPublicRoute('devices')).toBe(false)
      expect(isPublicRoute('device-detail')).toBe(false)
      expect(isPublicRoute('settings')).toBe(false)
      expect(isPublicRoute('dashboard')).toBe(false)
    })
  })

  describe('$currentRoute', () => {
    it('defaults to home', () => {
      expect($currentRoute.get()).toBe('home')
    })

    it('can be updated via setCurrentRoute', () => {
      setCurrentRoute('devices')
      expect($currentRoute.get()).toBe('devices')
    })
  })

  describe('$navCollapsed', () => {
    it('defaults to false', () => {
      expect($navCollapsed.get()).toBe(false)
    })

    it('toggles via toggleNav', () => {
      toggleNav()
      expect($navCollapsed.get()).toBe(true)
      toggleNav()
      expect($navCollapsed.get()).toBe(false)
    })
  })

  describe('$searchQuery', () => {
    it('can be updated via setSearchQuery', () => {
      setSearchQuery('modbus')
      expect($searchQuery.get()).toBe('modbus')
    })
  })

  describe('$alarmCount', () => {
    it('can be updated via setAlarmCount', () => {
      setAlarmCount(5)
      expect($alarmCount.get()).toBe(5)
    })
  })

  describe('notifications', () => {
    it('starts empty', () => {
      expect($notifications.get()).toEqual([])
    })

    it('addNotification prepends with id and timestamp', () => {
      addNotification({
        type: 'warning',
        title: 'High CPU',
        message: 'CPU usage exceeded 90%',
      })

      const notifications = $notifications.get()
      expect(notifications).toHaveLength(1)
      expect(notifications[0]).toMatchObject({
        id: 'test-uuid-123',
        type: 'warning',
        title: 'High CPU',
        message: 'CPU usage exceeded 90%',
        read: false,
      })
      expect(notifications[0].timestamp).toMatch(/^\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}/)
    })

    it('addNotification prepends to existing notifications', () => {
      addNotification({ type: 'info', title: 'First', message: 'msg' })
      addNotification({ type: 'danger', title: 'Second', message: 'msg2' })

      const notifications = $notifications.get()
      expect(notifications).toHaveLength(2)
      expect(notifications[0].title).toBe('Second')
      expect(notifications[1].title).toBe('First')
    })

    it('markNotificationRead updates specific notification', () => {
      crypto.randomUUID = vi.fn()
        .mockReturnValueOnce('uuid-1')
        .mockReturnValueOnce('uuid-2')

      addNotification({ type: 'info', title: 'First', message: 'msg1' })
      addNotification({ type: 'info', title: 'Second', message: 'msg2' })

      markNotificationRead('uuid-1')

      const notifications = $notifications.get()
      expect(notifications[1].read).toBe(true)
      expect(notifications[0].read).toBe(false)
    })

    it('clearNotifications empties the list', () => {
      addNotification({ type: 'info', title: 'Test', message: 'msg' })
      clearNotifications()
      expect($notifications.get()).toEqual([])
    })
  })
})
