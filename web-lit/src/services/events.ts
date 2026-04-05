/**
 * 事件服务 - Pure async API functions
 */

import { apiGet, apiPost } from '../lib/api-client'
import type { PaginatedResponse } from '../lib/api-client'

// Types
export const EventLevel = {
  Debug: 1,
  Info: 2,
  Warning: 3,
  Error: 4,
  Critical: 5,
} as const

export type EventLevelValue = typeof EventLevel[keyof typeof EventLevel]

export interface EventType {
  System: {
    UserAuth?: null
    UserOperation?: null
    SystemConfig?: null
    SystemError?: null
  } | {
    Device: {
      Connection?: null
      Property?: null
      Command?: null
      Business?: null
    }
  }
}

export interface ContentElement {
  Text?: {
    content: string
    format: 'Plain' | 'Markdown' | 'Html'
  }
  Image?: {
    url?: string
    base64?: string
    altText: string
    width?: number
    height?: number
  }
  Link?: {
    url: string
    text: string
    target: 'Self' | 'Blank' | 'Parent' | 'Top'
  }
  Table?: {
    headers: string[]
    rows: string[][]
  }
  Code?: {
    content: string
    language?: string
  }
}

export interface RichContent {
  title: string
  elements: ContentElement[]
  metadata: Record<string, any>
}

export interface EventSource {
  System?: {
    component: string
    userId?: string
  }
  Device?: {
    deviceId: string
    driverId?: string
  }
}

export interface Event {
  id: string
  eventType: EventType
  eventLevel: number
  timestamp: string
  source: EventSource
  content: RichContent
  createdAt: string
}

export interface RealTimeEvent {
  id: string
  eventType: EventType
  eventLevel: number
  sourceType: string
  sourceId: string
  deviceId?: string
  propertyId?: string
  title: string
  content: RichContent
  firstOccurrence: string
  lastUpdate: string
  occurrenceCount: number
  acknowledged: boolean
  acknowledgedBy?: string
  acknowledgedAt?: string
}

export interface EventQuery {
  startTime?: string
  endTime?: string
  eventTypes?: string[]
  eventLevels?: number[]
  deviceIds?: string[]
  userIds?: string[]
  keywords?: string
  page?: number
  pageSize?: number
  sortBy?: 'timestamp' | 'level' | 'type'
  sortOrder?: 'asc' | 'desc'
}

export interface RealTimeFilter {
  eventLevels?: number[]
  eventTypes?: string[]
  deviceIds?: string[]
  acknowledged?: boolean
}

export interface EventOverview {
  totalEvents: number
  eventsByLevel: Record<string, number>
  eventsByType: Record<string, number>
  recentEvents: Event[]
  activeAlertsCount: number
  trendsData: {
    timestamp: string
    count: number
    level: number
  }[]
}

export interface StatusSummary {
  totalActiveEvents: number
  criticalCount: number
  errorCount: number
  warningCount: number
  infoCount: number
  debugCount: number
  acknowledgedCount: number
  unacknowledgedCount: number
}

// Pure async API functions
export const eventApi = {
  getEvents: (params?: EventQuery) =>
    apiGet<PaginatedResponse<Event>>('events', params),

  getRealTimeEvents: (filter?: RealTimeFilter) =>
    apiGet<RealTimeEvent[]>('events/real-time', filter),

  getEventOverview: (params?: { startTime?: string; endTime?: string }) =>
    apiGet<EventOverview>('events/overview', params),

  getStatusSummary: () =>
    apiGet<StatusSummary>('events/real-time/summary'),

  acknowledgeEvent: (id: string) =>
    apiPost<boolean>(`events/real-time/${id}/acknowledge`, {}),

  batchAcknowledgeEvents: (ids: string[]) =>
    apiPost<boolean>('events/real-time/batch/acknowledge', { ids }),

  getEvent: (id: string) =>
    apiGet<Event>(`events/${id}`),

  exportEvents: async (params?: EventQuery & { format: 'json' | 'csv' }): Promise<Blob> => {
    const url = new URL('events/export', window.location.origin)
    if (params) {
      Object.entries(params).forEach(([key, value]) => {
        if (value !== undefined && value !== null) {
          url.searchParams.append(key, String(value))
        }
      })
    }
    const token = sessionStorage.getItem('auth-token')
    const response = await fetch(url.toString(), {
      credentials: 'include',
      headers: token ? { 'Authorization': `Bearer ${token}` } : {},
    })
    if (!response.ok) throw new Error(`Export failed: ${response.status}`)
    return response.blob()
  },
}

// Utility functions
export const getEventLevelName = (level: number): string => {
  switch (level) {
    case 5: return 'Critical'
    case 4: return 'Error'
    case 3: return 'Warning'
    case 2: return 'Info'
    case 1: return 'Debug'
    default: return 'Unknown'
  }
}

export const getEventLevelColor = (level: number): string => {
  switch (level) {
    case 5: return 'red'
    case 4: return 'orange'
    case 3: return 'yellow'
    case 2: return 'blue'
    case 1: return 'gray'
    default: return 'gray'
  }
}

export const getEventTypeName = (eventType: EventType): string => {
  if ('System' in eventType) {
    const systemType = eventType.System
    if ('UserAuth' in systemType) return 'User Authentication'
    if ('UserOperation' in systemType) return 'User Operation'
    if ('SystemConfig' in systemType) return 'System Configuration'
    if ('SystemError' in systemType) return 'System Error'
  }

  if ('Device' in eventType) {
    const deviceType = eventType.Device as any
    if ('Connection' in deviceType) return 'Device Connection'
    if ('Property' in deviceType) return 'Device Property'
    if ('Command' in deviceType) return 'Device Command'
    if ('Business' in deviceType) return 'Device Business'
  }

  return 'Unknown'
}

export const formatEventContent = (content: RichContent): string => {
  return content.elements
    .map(element => {
      if ('Text' in element) {
        return element.Text?.content || ''
      }
      if ('Code' in element) {
        return element.Code?.content || ''
      }
      if ('Table' in element) {
        return `Table: ${element.Table?.headers.join(', ')}`
      }
      if ('Link' in element) {
        return element.Link?.text || element.Link?.url || ''
      }
      if ('Image' in element) {
        return element.Image?.altText || 'Image'
      }
      return ''
    })
    .filter(text => text.length > 0)
    .join(' ')
}
