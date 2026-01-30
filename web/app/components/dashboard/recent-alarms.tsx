'use client'

import React from 'react'
import { RiAlarmWarningLine, RiCheckLine, RiTimeLine } from '@remixicon/react'
import type { RecentAlarm } from '@/types'
import cn from '@/utils/classnames'

interface RecentAlarmsProps {
  alarms: RecentAlarm[] | null
  loading?: boolean
}

const RecentAlarms = ({ alarms, loading }: RecentAlarmsProps) => {
  const getLevelColor = (level: string) => {
    switch (level) {
      case 'critical':
        return 'text-text-destructive bg-components-badge-bg-red-soft'
      case 'error':
        return 'text-text-destructive bg-components-badge-bg-red-soft'
      case 'warning':
        return 'text-text-warning bg-components-badge-bg-yellow-soft'
      case 'info':
        return 'text-text-accent bg-components-badge-bg-blue-soft'
      default:
        return 'text-text-tertiary bg-components-badge-bg-gray-soft'
    }
  }

  const getLevelText = (level: string) => {
    switch (level) {
      case 'critical':
        return '严重'
      case 'error':
        return '错误'
      case 'warning':
        return '警告'
      case 'info':
        return '信息'
      default:
        return '未知'
    }
  }

  const getStatusIcon = (status: string) => {
    switch (status) {
      case 'resolved':
        return <RiCheckLine className="w-4 h-4 text-text-success" />
      case 'acknowledged':
        return <RiTimeLine className="w-4 h-4 text-text-warning" />
      case 'active':
        return <RiAlarmWarningLine className="w-4 h-4 text-text-destructive" />
      default:
        return <RiAlarmWarningLine className="w-4 h-4 text-text-tertiary" />
    }
  }

  const getStatusText = (status: string) => {
    switch (status) {
      case 'resolved':
        return '已解决'
      case 'acknowledged':
        return '已确认'
      case 'active':
        return '活跃'
      default:
        return '未知'
    }
  }

  const formatTime = (timestamp: string) => {
    try {
      const date = new Date(timestamp)
      const now = new Date()
      const diff = now.getTime() - date.getTime()
      
      if (diff < 60000) { // 1分钟内
        return '刚刚'
      } else if (diff < 3600000) { // 1小时内
        return `${Math.floor(diff / 60000)}分钟前`
      } else if (diff < 86400000) { // 24小时内
        return `${Math.floor(diff / 3600000)}小时前`
      } else {
        return date.toLocaleDateString('zh-CN', {
          month: '2-digit',
          day: '2-digit',
          hour: '2-digit',
          minute: '2-digit'
        })
      }
    } catch {
      return '时间未知'
    }
  }

  if (loading) {
    return (
      <div className="bg-components-panel-bg shadow rounded-lg border border-divider-subtle">
        <div className="px-4 py-5 sm:p-6">
          <h3 className="text-lg leading-6 font-medium text-text-primary mb-4">
            最新告警
          </h3>
          <div className="space-y-4">
            {[...Array(5)].map((_, i) => (
              <div key={i} className="animate-pulse">
                <div className="flex items-start space-x-3">
                  <div className="w-8 h-8 bg-components-panel-bg-alt rounded-full"></div>
                  <div className="flex-1 space-y-2">
                    <div className="h-4 bg-components-panel-bg-alt rounded w-3/4"></div>
                    <div className="h-3 bg-components-panel-bg-alt rounded w-1/2"></div>
                  </div>
                  <div className="h-6 bg-components-panel-bg-alt rounded w-12"></div>
                </div>
              </div>
            ))}
          </div>
        </div>
      </div>
    )
  }

  return (
    <div className="bg-components-panel-bg shadow rounded-lg border border-divider-subtle">
      <div className="px-4 py-5 sm:p-6">
        <div className="flex items-center justify-between mb-4">
          <h3 className="text-lg leading-6 font-medium text-text-primary">
            最新告警
          </h3>
          <button className="text-sm text-components-button-primary-bg hover:text-components-button-primary-bg-hover">
            查看全部
          </button>
        </div>
        
        {!alarms || alarms.length === 0 ? (
          <div className="text-center py-8 text-text-tertiary">
            <RiCheckLine className="w-12 h-12 mx-auto mb-2 text-text-success" />
            <div className="text-sm">暂无活跃告警</div>
          </div>
        ) : (
          <div className="space-y-4">
            {alarms.slice(0, 8).map((alarm) => (
              <div key={alarm.id} className="flex items-start space-x-3 p-3 rounded-lg hover:bg-components-panel-bg-alt transition-colors">
                <div className="flex-shrink-0 mt-0.5">
                  {getStatusIcon(alarm.status)}
                </div>
                <div className="flex-1 min-w-0">
                  <div className="flex items-center space-x-2 mb-1">
                    <span className={cn(
                      "inline-flex items-center px-2 py-0.5 rounded text-xs font-medium",
                      getLevelColor(alarm.level)
                    )}>
                      {getLevelText(alarm.level)}
                    </span>
                    <span className="text-xs text-text-tertiary">
                      {getStatusText(alarm.status)}
                    </span>
                  </div>
                  <p className="text-sm font-medium text-text-primary truncate">
                    {alarm.deviceName}
                  </p>
                  <p className="text-sm text-text-secondary line-clamp-2">
                    {alarm.message}
                  </p>
                </div>
                <div className="flex-shrink-0 text-xs text-text-tertiary">
                  {formatTime(alarm.createdAt)}
                </div>
              </div>
            ))}
          </div>
        )}
      </div>
    </div>
  )
}

export default RecentAlarms