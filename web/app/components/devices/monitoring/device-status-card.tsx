'use client'

import React from 'react'
import { 
  RiWifiLine, 
  RiWifiOffLine,
  RiSignalWifiLine,
  RiDeviceLine,
  RiCommandLine,
  RiNotificationLine,
  RiAlarmWarningLine
} from '@remixicon/react'
import type { DeviceOnlineStatus, DeviceMetrics } from '@/service/device-monitoring'
import cn from '@/utils/classnames'

interface DeviceStatusCardProps {
  deviceStatus?: DeviceOnlineStatus | null
  deviceMetrics?: DeviceMetrics | null
  loading?: boolean
}

const DeviceStatusCard = ({ deviceStatus, deviceMetrics, loading }: DeviceStatusCardProps) => {
  const getConnectionQualityColor = (quality?: number) => {
    if (!quality) return 'text-text-tertiary'
    if (quality >= 80) return 'text-text-success'
    if (quality >= 60) return 'text-text-warning'
    return 'text-text-destructive'
  }

  const getConnectionQualityText = (quality?: number) => {
    if (!quality) return '未知'
    if (quality >= 80) return '优秀'
    if (quality >= 60) return '良好'
    if (quality >= 40) return '一般'
    return '较差'
  }

  const formatLastCheck = (lastCheck?: string) => {
    if (!lastCheck) return '--'
    try {
      const date = new Date(lastCheck)
      const now = new Date()
      const diffMs = now.getTime() - date.getTime()
      const diffMinutes = Math.floor(diffMs / (1000 * 60))
      
      if (diffMinutes < 1) return '刚刚'
      if (diffMinutes < 60) return `${diffMinutes}分钟前`
      if (diffMinutes < 1440) return `${Math.floor(diffMinutes / 60)}小时前`
      return `${Math.floor(diffMinutes / 1440)}天前`
    } catch {
      return lastCheck
    }
  }

  if (loading) {
    return (
      <div className="bg-components-panel-bg shadow rounded-lg border border-divider-subtle">
        <div className="px-4 py-5 sm:p-6">
          <h3 className="text-lg leading-6 font-medium text-text-primary mb-4">
            设备状态
          </h3>
          <div className="space-y-4 animate-pulse">
            <div className="flex items-center justify-between">
              <div className="flex items-center space-x-3">
                <div className="w-8 h-8 bg-components-panel-bg-alt rounded-full"></div>
                <div className="space-y-2">
                  <div className="h-4 bg-components-panel-bg-alt rounded w-16"></div>
                  <div className="h-3 bg-components-panel-bg-alt rounded w-24"></div>
                </div>
              </div>
            </div>
            <div className="grid grid-cols-2 gap-4">
              {[...Array(4)].map((_, i) => (
                <div key={i} className="space-y-2">
                  <div className="h-3 bg-components-panel-bg-alt rounded w-12"></div>
                  <div className="h-6 bg-components-panel-bg-alt rounded w-8"></div>
                </div>
              ))}
            </div>
          </div>
        </div>
      </div>
    )
  }

  return (
    <div className="bg-components-panel-bg shadow rounded-lg border border-divider-subtle">
      <div className="px-4 py-5 sm:p-6">
        <h3 className="text-lg leading-6 font-medium text-text-primary mb-4">
          设备状态
        </h3>
        
        <div className="space-y-4">
          {/* 在线状态 */}
          <div className="flex items-center justify-between">
            <div className="flex items-center space-x-3">
              <div className={cn(
                "w-8 h-8 rounded-full flex items-center justify-center",
                deviceStatus?.isOnline ? "bg-text-success" : "bg-text-tertiary"
              )}>
                {deviceStatus?.isOnline ? (
                  <RiWifiLine className="w-4 h-4 text-white" />
                ) : (
                  <RiWifiOffLine className="w-4 h-4 text-white" />
                )}
              </div>
              <div>
                <div className="text-sm font-medium text-text-primary">
                  {deviceStatus?.isOnline ? '在线' : '离线'}
                </div>
                <div className="text-xs text-text-tertiary">
                  最后检查: {formatLastCheck(deviceStatus?.lastCheck)}
                </div>
              </div>
            </div>
            
            {/* 连接质量 */}
            {deviceStatus?.connectionQuality && (
              <div className="text-right">
                <div className={cn(
                  "text-sm font-medium flex items-center",
                  getConnectionQualityColor(deviceStatus.connectionQuality)
                )}>
                  <RiSignalWifiLine className="w-4 h-4 mr-1" />
                  {deviceStatus.connectionQuality}%
                </div>
                <div className="text-xs text-text-tertiary">
                  {getConnectionQualityText(deviceStatus.connectionQuality)}
                </div>
              </div>
            )}
          </div>

          {/* 设备统计 */}
          <div className="border-t border-divider-subtle pt-4">
            <div className="grid grid-cols-2 gap-4">
              <div className="space-y-1">
                <div className="flex items-center text-xs text-text-tertiary">
                  <RiDeviceLine className="w-3 h-3 mr-1" />
                  属性数量
                </div>
                <div className="text-lg font-semibold text-text-primary">
                  {deviceMetrics?.totalProperties || 0}
                </div>
                <div className="text-xs text-text-tertiary">
                  在线: {deviceMetrics?.onlineProperties || 0}
                </div>
              </div>

              <div className="space-y-1">
                <div className="flex items-center text-xs text-text-tertiary">
                  <RiCommandLine className="w-3 h-3 mr-1" />
                  指令数量
                </div>
                <div className="text-lg font-semibold text-text-primary">
                  {deviceMetrics?.totalCommands || 0}
                </div>
              </div>

              <div className="space-y-1">
                <div className="flex items-center text-xs text-text-tertiary">
                  <RiNotificationLine className="w-3 h-3 mr-1" />
                  事件数量
                </div>
                <div className="text-lg font-semibold text-text-primary">
                  {deviceMetrics?.totalEvents || 0}
                </div>
              </div>

              <div className="space-y-1">
                <div className="flex items-center text-xs text-text-tertiary">
                  <RiAlarmWarningLine className="w-3 h-3 mr-1" />
                  活跃告警
                </div>
                <div className={cn(
                  "text-lg font-semibold",
                  (deviceMetrics?.activeAlarms || 0) > 0 ? "text-text-destructive" : "text-text-primary"
                )}>
                  {deviceMetrics?.activeAlarms || 0}
                </div>
              </div>
            </div>
          </div>
        </div>
      </div>
    </div>
  )
}

export default DeviceStatusCard