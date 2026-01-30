'use client'

import React from 'react'
import { useEventOverview, useRealTimeEvents, useStatusSummary } from '@/service/events'
import { getEventLevelName, getEventLevelColor, getEventTypeName, formatEventContent } from '@/service/events'

interface EventDashboardProps {
  className?: string
}

export const EventDashboard: React.FC<EventDashboardProps> = ({ className = '' }) => {
  const { data: overview, isLoading: overviewLoading } = useEventOverview()
  const { data: realTimeEvents, isLoading: realTimeLoading } = useRealTimeEvents()
  const { data: statusSummary, isLoading: statusLoading } = useStatusSummary()

  if (overviewLoading || realTimeLoading || statusLoading) {
    return (
      <div className={`p-6 ${className}`}>
        <div className="animate-pulse">
          <div className="h-8 bg-gray-200 rounded mb-4"></div>
          <div className="grid grid-cols-1 md:grid-cols-3 gap-4 mb-6">
            {[1, 2, 3].map(i => (
              <div key={i} className="h-24 bg-gray-200 rounded"></div>
            ))}
          </div>
          <div className="h-64 bg-gray-200 rounded"></div>
        </div>
      </div>
    )
  }

  return (
    <div className={`p-6 ${className}`}>
      <h2 className="text-2xl font-bold mb-6">Event System Dashboard</h2>
      
      {/* Status Summary Cards */}
      {statusSummary && (
        <div className="grid grid-cols-1 md:grid-cols-3 lg:grid-cols-6 gap-4 mb-6">
          <StatusCard
            title="Total Active"
            value={statusSummary.totalActiveEvents}
            color="blue"
          />
          <StatusCard
            title="Critical"
            value={statusSummary.criticalCount}
            color="red"
          />
          <StatusCard
            title="Error"
            value={statusSummary.errorCount}
            color="orange"
          />
          <StatusCard
            title="Warning"
            value={statusSummary.warningCount}
            color="yellow"
          />
          <StatusCard
            title="Info"
            value={statusSummary.infoCount}
            color="blue"
          />
          <StatusCard
            title="Debug"
            value={statusSummary.debugCount}
            color="gray"
          />
        </div>
      )}

      {/* Overview Statistics */}
      {overview && (
        <div className="grid grid-cols-1 lg:grid-cols-2 gap-6 mb-6">
          <div className="bg-white rounded-lg shadow p-6">
            <h3 className="text-lg font-semibold mb-4">Events by Level</h3>
            <div className="space-y-2">
              {Object.entries(overview.eventsByLevel).map(([level, count]) => (
                <div key={level} className="flex justify-between items-center">
                  <span className="text-sm text-gray-600">{level}</span>
                  <span className="font-medium">{count}</span>
                </div>
              ))}
            </div>
          </div>
          
          <div className="bg-white rounded-lg shadow p-6">
            <h3 className="text-lg font-semibold mb-4">Events by Type</h3>
            <div className="space-y-2">
              {Object.entries(overview.eventsByType).map(([type, count]) => (
                <div key={type} className="flex justify-between items-center">
                  <span className="text-sm text-gray-600">{type}</span>
                  <span className="font-medium">{count}</span>
                </div>
              ))}
            </div>
          </div>
        </div>
      )}

      {/* Real-time Events */}
      <div className="bg-white rounded-lg shadow">
        <div className="p-6 border-b">
          <h3 className="text-lg font-semibold">Active Events</h3>
          <p className="text-sm text-gray-600">Events currently requiring attention</p>
        </div>
        
        <div className="divide-y">
          {realTimeEvents && realTimeEvents.length > 0 ? (
            realTimeEvents.slice(0, 10).map((event) => (
              <EventItem key={event.id} event={event} />
            ))
          ) : (
            <div className="p-6 text-center text-gray-500">
              No active events
            </div>
          )}
        </div>
      </div>

      {/* Recent Events */}
      {overview?.recentEvents && overview.recentEvents.length > 0 && (
        <div className="bg-white rounded-lg shadow mt-6">
          <div className="p-6 border-b">
            <h3 className="text-lg font-semibold">Recent Events</h3>
            <p className="text-sm text-gray-600">Latest system events</p>
          </div>
          
          <div className="divide-y">
            {overview.recentEvents.slice(0, 5).map((event) => (
              <RecentEventItem key={event.id} event={event} />
            ))}
          </div>
        </div>
      )}
    </div>
  )
}

interface StatusCardProps {
  title: string
  value: number
  color: 'red' | 'orange' | 'yellow' | 'blue' | 'gray'
}

const StatusCard: React.FC<StatusCardProps> = ({ title, value, color }) => {
  const colorClasses = {
    red: 'bg-red-50 text-red-700 border-red-200',
    orange: 'bg-orange-50 text-orange-700 border-orange-200',
    yellow: 'bg-yellow-50 text-yellow-700 border-yellow-200',
    blue: 'bg-blue-50 text-blue-700 border-blue-200',
    gray: 'bg-gray-50 text-gray-700 border-gray-200',
  }

  return (
    <div className={`p-4 rounded-lg border ${colorClasses[color]}`}>
      <div className="text-2xl font-bold">{value}</div>
      <div className="text-sm">{title}</div>
    </div>
  )
}

interface EventItemProps {
  event: any // RealTimeEvent type
}

const EventItem: React.FC<EventItemProps> = ({ event }) => {
  const levelColor = getEventLevelColor(event.eventLevel)
  const levelName = getEventLevelName(event.eventLevel)
  const typeName = getEventTypeName(event.eventType)
  const content = formatEventContent(event.content)

  return (
    <div className="p-4 hover:bg-gray-50">
      <div className="flex items-start justify-between">
        <div className="flex-1">
          <div className="flex items-center gap-2 mb-1">
            <span className={`inline-block w-2 h-2 rounded-full bg-${levelColor}-500`}></span>
            <span className="font-medium text-sm">{event.title}</span>
            <span className="text-xs text-gray-500">{levelName}</span>
            <span className="text-xs text-gray-500">•</span>
            <span className="text-xs text-gray-500">{typeName}</span>
          </div>
          
          <p className="text-sm text-gray-600 mb-2 line-clamp-2">
            {content}
          </p>
          
          <div className="flex items-center gap-4 text-xs text-gray-500">
            <span>First: {new Date(event.firstOccurrence).toLocaleString()}</span>
            <span>Last: {new Date(event.lastUpdate).toLocaleString()}</span>
            <span>Count: {event.occurrenceCount}</span>
            {event.deviceId && <span>Device: {event.deviceId}</span>}
          </div>
        </div>
        
        <div className="flex items-center gap-2 ml-4">
          {!event.acknowledged && (
            <button className="text-xs bg-blue-100 text-blue-700 px-2 py-1 rounded hover:bg-blue-200">
              Acknowledge
            </button>
          )}
          {event.acknowledged && (
            <span className="text-xs text-green-600">✓ Acknowledged</span>
          )}
        </div>
      </div>
    </div>
  )
}

interface RecentEventItemProps {
  event: any // Event type
}

const RecentEventItem: React.FC<RecentEventItemProps> = ({ event }) => {
  const levelColor = getEventLevelColor(event.eventLevel)
  const levelName = getEventLevelName(event.eventLevel)
  const typeName = getEventTypeName(event.eventType)
  const content = formatEventContent(event.content)

  return (
    <div className="p-4 hover:bg-gray-50">
      <div className="flex items-start gap-3">
        <span className={`inline-block w-2 h-2 rounded-full bg-${levelColor}-500 mt-2`}></span>
        
        <div className="flex-1">
          <div className="flex items-center gap-2 mb-1">
            <span className="font-medium text-sm">{event.content.title}</span>
            <span className="text-xs text-gray-500">{levelName}</span>
            <span className="text-xs text-gray-500">•</span>
            <span className="text-xs text-gray-500">{typeName}</span>
          </div>
          
          <p className="text-sm text-gray-600 mb-1 line-clamp-1">
            {content}
          </p>
          
          <span className="text-xs text-gray-500">
            {new Date(event.timestamp).toLocaleString()}
          </span>
        </div>
      </div>
    </div>
  )
}

export default EventDashboard