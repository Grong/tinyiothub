/**
 * App State - Centralized application state store
 *
 * Provides a single source of truth for:
 * - Authentication state (user, token)
 * - Navigation state (current route, sidebar collapsed)
 * - UI state (notifications, search)
 * - Global actions (navigate, logout)
 */

import { atom, computed } from 'nanostores'
import type { User } from './auth-store'

// Route state
export const $currentRoute = atom<string>('home')
export const $navCollapsed = atom<boolean>(false)

// Search state
export const $searchQuery = atom<string>('')

// Notification state
export const $alarmCount = atom<number>(0)
export const $notifications = atom<Notification[]>([])

// Computed: is public route
export const PUBLIC_ROUTES = ['home', 'signin', 'register', 'tenant/register', 'marketplace', 'installed-marketplace']

export function isPublicRoute(route: string): boolean {
  return PUBLIC_ROUTES.includes(route)
}

// Notification types
export interface Notification {
  id: string
  type: 'info' | 'warning' | 'danger'
  title: string
  message: string
  timestamp: string
  read: boolean
}

// Actions
export function setCurrentRoute(route: string) {
  $currentRoute.set(route)
}

export function toggleNav() {
  $navCollapsed.set(!$navCollapsed.get())
}

export function setSearchQuery(query: string) {
  $searchQuery.set(query)
}

export function setAlarmCount(count: number) {
  $alarmCount.set(count)
}

export function addNotification(notification: Omit<Notification, 'id' | 'timestamp' | 'read'>) {
  const notifications = $notifications.get()
  const newNotification: Notification = {
    ...notification,
    id: crypto.randomUUID(),
    timestamp: new Date().toISOString(),
    read: false,
  }
  $notifications.set([newNotification, ...notifications])
}

export function markNotificationRead(id: string) {
  const notifications = $notifications.get()
  $notifications.set(
    notifications.map((n) => (n.id === id ? { ...n, read: true } : n))
  )
}

export function clearNotifications() {
  $notifications.set([])
}
