'use client'

import React from 'react'
import { 
  RiTimeLine, 
  RiPulseLine, 
  RiAlertLine, 
  RiInformationLine, 
  RiAlarmWarningLine, 
  RiCloseCircleLine 
} from '@remixicon/react'
import type { DeviceEventSummary } from '@/service/devices'

interface DeviceEventsProps {
  events: DeviceEventSummary[]
  isLoading?: boolean
}

// 事件级别对应的图标和颜色
const getLevelConfig = (level: string) => {
  switch (level.toLowerCase()) {
    case 'critical':
      return {
        icon: RiCloseCircleLine,
        color: 'text-red-600',
        bgColor: 'bg-red-50',
        borderColor: 'border-red-200',
      }
    case 'error':
      return {
        icon: RiAlertLine,
        color: 'text-red-500',
        bgColor: 'bg-red-50',
        borderColor: 'border-red-200',
      }
    case 'warning':
      return {
        icon: RiAlarmWarningLine,
        color: 'text-yellow-600',
        bgColor: 'bg-yellow-50',
        borderColor: 'border-yellow-200',
      }
    case 'info':
      return {
        icon: RiInformationLine,
        color: 'text-blue-600',
        bgColor: 'bg-blue-50',
        borderColor: 'border-blue-200',
      }
    case 'debug':
      return {
        icon: RiPulseLine,
        color: 'text-gray-600',
        bgColor: 'bg-gray-50',
        borderColor: 'border-gray-200',
      }
    default:
      return {
        icon: RiPulseLine,
        color: 'text-gray-600',
        bgColor: 'bg-gray-50',
        borderColor: 'border-gray-200',
      }
  }
}

// 事件类型对应的标签
const getEventTypeLabel = (eventType: string) => {
  switch (eventType) {
    case 'Connection':
      return '连接'
    case 'Property':
      return '属性'
    case 'Command':
      return '命令'
    case 'Business':
      return '业务'
    case 'System':
      return '系统'
    default:
      return eventType
  }
}

export const DeviceEvents: React.FC<DeviceEventsProps> = ({ events, isLoading }) => {
  if (isLoading) {
    return (
      <div className="space-y-3">
        {[...Array(3)].map((_, i) => (
          <div key={i} className="animate-pulse">
            <div className="h-20 bg-gray-100 rounded-lg"></div>
          </div>
        ))}
      </div>
    )
  }

  if (!events || events.length === 0) {
    return (
      <div className="text-center py-8 text-gray-500">
        <RiPulseLine className="w-12 h-12 mx-auto mb-2 opacity-50" />
        <p>暂无事件记录</p>
      </div>
    )
  }

  return (
    <div className="space-y-2">
      {events.map((event) => {
        const levelConfig = getLevelConfig(event.level)
        const Icon = levelConfig.icon

        return (
          <div
            key={event.id}
            className={`p-4 rounded-lg border ${levelConfig.borderColor} ${levelConfig.bgColor} hover:shadow-sm transition-shadow`}
          >
            <div className="flex items-start gap-3">
              {/* 图标 */}
              <div className={`flex-shrink-0 ${levelConfig.color}`}>
                <Icon className="w-5 h-5" />
              </div>

              {/* 内容 */}
              <div className="flex-1 min-w-0">
                {/* 标题和类型 */}
                <div className="flex items-center gap-2 mb-1">
                  <h4 className="font-medium text-gray-900 truncate">
                    {event.title}
                  </h4>
                  <span className="flex-shrink-0 px-2 py-0.5 text-xs font-medium bg-white border border-gray-200 rounded">
                    {getEventTypeLabel(event.eventType)}
                  </span>
                </div>

                {/* 消息 */}
                <p className="text-sm text-gray-600 mb-2">
                  {event.message}
                </p>

                {/* 元数据 */}
                {event.metadata && Object.keys(event.metadata).length > 0 && (
                  <div className="flex flex-wrap gap-2 mb-2">
                    {Object.entries(event.metadata).map(([key, value]) => {
                      // 跳过一些内部字段
                      if (key.startsWith('_')) return null
                      
                      return (
                        <span
                          key={key}
                          className="text-xs px-2 py-1 bg-white border border-gray-200 rounded"
                        >
                          <span className="text-gray-500">{key}:</span>{' '}
                          <span className="text-gray-700 font-medium">
                            {typeof value === 'object' ? JSON.stringify(value) : String(value)}
                          </span>
                        </span>
                      )
                    })}
                  </div>
                )}

                {/* 时间 */}
                <div className="flex items-center gap-1 text-xs text-gray-500">
                  <RiTimeLine className="w-3 h-3" />
                  <span>{event.timestamp}</span>
                </div>
              </div>

              {/* 级别标签 */}
              <div className="flex-shrink-0">
                <span
                  className={`px-2 py-1 text-xs font-medium rounded ${levelConfig.color} ${levelConfig.bgColor}`}
                >
                  {event.level}
                </span>
              </div>
            </div>
          </div>
        )
      })}
    </div>
  )
}
