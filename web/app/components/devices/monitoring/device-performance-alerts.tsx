'use client'

import React from 'react'
import { 
  RiAlarmWarningLine,
  RiErrorWarningLine,
  RiInformationLine,
  RiTimeLine,
  RiCpuLine,
  RiRamLine,
  RiWifiLine,
  RiHeartPulseLine
} from '@remixicon/react'
import type { PerformanceAlert } from '@/service/device-monitoring'
import cn from '@/utils/classnames'

interface DevicePerformanceAlertsProps {
  alerts?: PerformanceAlert[] | null
  loading?: boolean
}

const DevicePerformanceAlerts = ({ alerts, loading }: DevicePerformanceAlertsProps) => {
  const getSeverityIcon = (severity: string) => {
    switch (severity) {
      case 'critical':
        return RiErrorWarningLine
      case 'warning':
        return RiAlarmWarningLine
      default:
        return RiInformationLine
    }
  }

  const getSeverityColor = (severity: string) => {
    switch (severity) {
      case 'critical':
        return 'text-text-destructive bg-components-panel-bg-alt border-divider-subtle'
      case 'warning':
        return 'text-text-warning bg-components-panel-bg-alt border-divider-subtle'
      default:
        return 'text-text-secondary bg-components-panel-bg-alt border-divider-subtle'
    }
  }

  const getSeverityBadgeColor = (severity: string) => {
    switch (severity) {
      case 'critical':
        return 'bg-text-destructive text-white'
      case 'warning':
        return 'bg-text-warning text-white'
      default:
        return 'bg-text-secondary text-white'
    }
  }

  const getAlertTypeIcon = (alertType: string) => {
    switch (alertType) {
      case 'high_cpu':
        return RiCpuLine
      case 'high_memory':
        return RiRamLine
      case 'high_latency':
      case 'slow_response':
        return RiWifiLine
      case 'high_error_rate':
        return RiErrorWarningLine
      case 'low_uptime':
        return RiHeartPulseLine
      default:
        return RiAlarmWarningLine
    }
  }

  const getAlertTypeName = (alertType: string) => {
    const typeMap: Record<string, string> = {
      'high_cpu': 'CPU使用率过高',
      'high_memory': '内存使用率过高',
      'high_latency': '网络延迟过高',
      'slow_response': '响应时间过长',
      'high_error_rate': '错误率过高',
      'low_uptime': '正常运行时间过低'
    }
    return typeMap[alertType] || alertType
  }

  const formatTimestamp = (timestamp: string) => {
    try {
      const date = new Date(timestamp)
      const now = new Date()
      const diffMs = now.getTime() - date.getTime()
      const diffMinutes = Math.floor(diffMs / (1000 * 60))
      
      if (diffMinutes < 1) return '刚刚'
      if (diffMinutes < 60) return `${diffMinutes}分钟前`
      if (diffMinutes < 1440) return `${Math.floor(diffMinutes / 60)}小时前`
      return date.toLocaleDateString('zh-CN') + ' ' + date.toLocaleTimeString('zh-CN', { 
        hour: '2-digit', 
        minute: '2-digit' 
      })
    } catch {
      return timestamp
    }
  }

  const formatValue = (value: number, alertType: string) => {
    if (value == null || typeof value !== 'number') return '--'
    
    switch (alertType) {
      case 'high_cpu':
      case 'high_memory':
      case 'high_error_rate':
      case 'low_uptime':
        return `${value.toFixed(1)}%`
      case 'high_latency':
      case 'slow_response':
        return `${value.toFixed(1)}ms`
      default:
        return value.toFixed(1)
    }
  }

  if (loading) {
    return (
      <div className="bg-components-panel-bg shadow rounded-lg border border-divider-subtle">
        <div className="px-4 py-5 sm:p-6">
          <h3 className="text-lg leading-6 font-medium text-text-primary mb-4">
            性能告警
          </h3>
          <div className="space-y-4 animate-pulse">
            {[...Array(3)].map((_, i) => (
              <div key={i} className="border border-divider-subtle rounded-lg p-4">
                <div className="flex items-start space-x-3">
                  <div className="w-8 h-8 bg-components-panel-bg-alt rounded-full"></div>
                  <div className="flex-1 space-y-2">
                    <div className="flex items-center justify-between">
                      <div className="h-4 bg-components-panel-bg-alt rounded w-32"></div>
                      <div className="h-5 bg-components-panel-bg-alt rounded w-16"></div>
                    </div>
                    <div className="h-3 bg-components-panel-bg-alt rounded w-full"></div>
                    <div className="h-3 bg-components-panel-bg-alt rounded w-24"></div>
                  </div>
                </div>
              </div>
            ))}
          </div>
        </div>
      </div>
    )
  }

  const criticalAlerts = alerts?.filter(alert => alert.severity === 'critical') || []
  const warningAlerts = alerts?.filter(alert => alert.severity === 'warning') || []

  return (
    <div className="bg-components-panel-bg shadow rounded-lg border border-divider-subtle">
      <div className="px-4 py-5 sm:p-6">
        <div className="flex items-center justify-between mb-4">
          <h3 className="text-lg leading-6 font-medium text-text-primary">
            性能告警
          </h3>
          <div className="flex items-center space-x-2">
            {criticalAlerts.length > 0 && (
              <span className="inline-flex items-center px-2.5 py-0.5 rounded-full text-xs font-medium bg-text-destructive text-white">
                {criticalAlerts.length} 严重
              </span>
            )}
            {warningAlerts.length > 0 && (
              <span className="inline-flex items-center px-2.5 py-0.5 rounded-full text-xs font-medium bg-text-warning text-white">
                {warningAlerts.length} 警告
              </span>
            )}
          </div>
        </div>

        {!alerts || alerts.length === 0 ? (
          <div className="text-center py-12">
            <RiAlarmWarningLine className="w-12 h-12 mx-auto mb-4 text-text-tertiary" />
            <div className="text-sm text-text-tertiary">暂无性能告警</div>
            <div className="text-xs text-text-tertiary mt-1">设备性能正常</div>
          </div>
        ) : (
          <div className="space-y-4">
            {alerts.map((alert, index) => {
              const SeverityIcon = getSeverityIcon(alert.severity)
              const AlertTypeIcon = getAlertTypeIcon(alert.alertType)
              
              return (
                <div
                  key={index}
                  className={cn(
                    "border rounded-lg p-4 transition-colors",
                    getSeverityColor(alert.severity)
                  )}
                >
                  <div className="flex items-start space-x-3">
                    <div className={cn(
                      "w-8 h-8 rounded-full flex items-center justify-center flex-shrink-0",
                      getSeverityBadgeColor(alert.severity)
                    )}>
                      <SeverityIcon className="w-4 h-4" />
                    </div>
                    
                    <div className="flex-1 min-w-0">
                      <div className="flex items-center justify-between mb-2">
                        <div className="flex items-center space-x-2">
                          <AlertTypeIcon className="w-4 h-4 text-text-secondary" />
                          <h4 className="text-sm font-medium text-text-primary">
                            {getAlertTypeName(alert.alertType)}
                          </h4>
                        </div>
                        <span className={cn(
                          "inline-flex items-center px-2 py-1 rounded-full text-xs font-medium",
                          getSeverityBadgeColor(alert.severity)
                        )}>
                          {alert.severity === 'critical' ? '严重' : '警告'}
                        </span>
                      </div>
                      
                      <p className="text-sm text-text-secondary mb-2">
                        {alert.message}
                      </p>
                      
                      <div className="flex items-center justify-between text-xs text-text-tertiary">
                        <div className="flex items-center space-x-4">
                          <span>
                            当前值: <span className="font-medium">
                              {formatValue(alert.currentValue, alert.alertType)}
                            </span>
                          </span>
                          <span>
                            阈值: <span className="font-medium">
                              {formatValue(alert.threshold, alert.alertType)}
                            </span>
                          </span>
                        </div>
                        <div className="flex items-center">
                          <RiTimeLine className="w-3 h-3 mr-1" />
                          {formatTimestamp(alert.timestamp)}
                        </div>
                      </div>
                    </div>
                  </div>
                </div>
              )
            })}
          </div>
        )}

        {alerts && alerts.length > 0 && (
          <div className="mt-6 pt-4 border-t border-divider-subtle">
            <div className="flex items-center justify-between text-sm">
              <div className="text-text-tertiary">
                共 {alerts.length} 个告警
              </div>
              <div className="flex items-center space-x-4">
                <div className="text-text-tertiary">
                  严重: <span className="font-medium text-text-destructive">
                    {criticalAlerts.length}
                  </span>
                </div>
                <div className="text-text-tertiary">
                  警告: <span className="font-medium text-text-warning">
                    {warningAlerts.length}
                  </span>
                </div>
              </div>
            </div>
          </div>
        )}
      </div>
    </div>
  )
}

export default DevicePerformanceAlerts