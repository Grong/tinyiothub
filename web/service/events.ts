// Event service for the new event system API
import { apiGet, apiPost, apiPut, apiDelete } from '@/lib/api-client'
import { useQuery, useMutation, useQueryClient } from '@tanstack/react-query'
import { queryKeys } from '@/lib/query-keys'

// Event types and interfaces
export interface EventLevel {
  Critical: number
  Error: number
  Warning: number
  Info: number
  Debug: number
}

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

export interface PaginatedResponse<T> {
  data: T[]
  pagination: {
    page: number
    pageSize: number
    totalPages: number
    totalCount: number
  }
}

// API functions
export const eventApi = {
  // Get events with filtering and pagination
  getEvents: (params?: EventQuery) => 
    apiGet<PaginatedResponse<Event>>('events', params),

  // Get real-time events
  getRealTimeEvents: (filter?: RealTimeFilter) => 
    apiGet<RealTimeEvent[]>('events/real-time', filter),

  // Get event overview/statistics
  getEventOverview: (params?: { startTime?: string; endTime?: string }) => 
    apiGet<EventOverview>('events/overview', params),

  // Get real-time status summary
  getStatusSummary: () => 
    apiGet<StatusSummary>('events/real-time/summary'),

  // Acknowledge a real-time event
  acknowledgeEvent: (id: string) => 
    apiPost<boolean>(`events/real-time/${id}/acknowledge`, {}),

  // Batch acknowledge events
  batchAcknowledgeEvents: (ids: string[]) => 
    apiPost<boolean>('events/real-time/batch/acknowledge', { ids }),

  // Get event by ID
  getEvent: (id: string) => 
    apiGet<Event>(`events/${id}`),

  // Export events
  exportEvents: (params?: EventQuery & { format: 'json' | 'csv' }) => 
    apiGet<Blob>('events/export', params),
}

// React Query hooks
export const useEvents = (params?: EventQuery) => {
  return useQuery({
    queryKey: queryKeys.events.list(params || {}),
    queryFn: async () => {
      const response = await eventApi.getEvents(params)
      return response.result || { data: [], pagination: { page: 1, pageSize: 20, totalPages: 0, totalCount: 0 } }
    },
  })
}

export const useRealTimeEvents = (filter?: RealTimeFilter) => {
  return useQuery({
    queryKey: queryKeys.events.realTime(filter || {}),
    queryFn: async () => {
      const response = await eventApi.getRealTimeEvents(filter)
      return response.result || []
    },
    refetchInterval: 5000, // Refresh every 5 seconds
  })
}

export const useEventOverview = (params?: { startTime?: string; endTime?: string }) => {
  return useQuery({
    queryKey: queryKeys.events.overview(params || {}),
    queryFn: async () => {
      const response = await eventApi.getEventOverview(params)
      return response.result
    },
    refetchInterval: 30000, // Refresh every 30 seconds
  })
}

export const useStatusSummary = () => {
  return useQuery({
    queryKey: queryKeys.events.statusSummary(),
    queryFn: async () => {
      const response = await eventApi.getStatusSummary()
      return response.result
    },
    refetchInterval: 10000, // Refresh every 10 seconds
  })
}

export const useEvent = (id: string, enabled = true) => {
  return useQuery({
    queryKey: queryKeys.events.detail(id),
    queryFn: async () => {
      const response = await eventApi.getEvent(id)
      return response.result
    },
    enabled: enabled && !!id,
  })
}

export const useAcknowledgeEvent = () => {
  const queryClient = useQueryClient()
  
  return useMutation({
    mutationFn: eventApi.acknowledgeEvent,
    onSuccess: () => {
      // Refresh real-time events and status summary
      queryClient.invalidateQueries({ queryKey: queryKeys.events.realTime({}) })
      queryClient.invalidateQueries({ queryKey: queryKeys.events.statusSummary() })
    },
  })
}

export const useBatchAcknowledgeEvents = () => {
  const queryClient = useQueryClient()
  
  return useMutation({
    mutationFn: eventApi.batchAcknowledgeEvents,
    onSuccess: () => {
      // Refresh real-time events and status summary
      queryClient.invalidateQueries({ queryKey: queryKeys.events.realTime({}) })
      queryClient.invalidateQueries({ queryKey: queryKeys.events.statusSummary() })
    },
  })
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
    const deviceType = eventType.Device
    if ('Connection' in deviceType) return 'Device Connection'
    if ('Property' in deviceType) return 'Device Property'
    if ('Command' in deviceType) return 'Device Command'
    if ('Business' in deviceType) return 'Device Business'
  }
  
  return 'Unknown'
}

export const formatEventContent = (content: RichContent): string => {
  // Simple text extraction from rich content for display
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