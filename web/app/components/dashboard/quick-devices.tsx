'use client'

import React from 'react'
import { RiDeviceLine, RiWifiLine, RiWifiOffLine, RiAlarmWarningLine, RiSettings3Line } from '@remixicon/react'
import type { QuickDevice } from '@/types'
import cn from '@/utils/classnames'

interface QuickDevicesProps {
  devices: QuickDevice[] | null
  loading?: boolean
}

const QuickDevices = ({ devices, loading }: QuickDevicesProps) => {
  const getStatusIcon = (status: string) => {
    switch (status) {
      case 'online':
        return <RiWifiLine className="w-4 h-4 text-text-success" />
      case 'offline':
        return <RiWifiOffLine className="w-4 h-4 text-text-tertiary" />
      case 'error':
        return <RiAlarmWarningLine className="w-4 h-4 text-text-destructive" />
      case 'maintenance':
        return <RiSettings3Line className="w-4 h-4 text-text-warning" />
      default:
        return <RiDeviceLine className="w-4 h-4 text-text-tertiary" />
    }
  }

  const getStatusColor = (status: string) => {
    switch (status) {
      case 'online':
        return 'bg-components-badge-bg-green-soft text-text-success'
      case 'offline':
        return 'bg-components-badge-bg-gray-soft text-text-tertiary'
      case 'error':
        return 'bg-components-badge-bg-red-soft text-text-destructive'
      case 'maintenance':
        return 'bg-components-badge-bg-yellow-soft text-text-warning'
      default:
        return 'bg-components-badge-bg-gray-soft text-text-tertiary'
    }
  }

  const getStatusText = (status: string) => {
    switch (status) {
      case 'online':
        return '在线'
      case 'offline':
        return '离线'
      case 'error':
        return '故障'
      case 'maintenance':
        return '维护'
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
          day: '2-digit'
        })
      }
    } catch {
      return '未知'
    }
  }

  if (loading) {
    return (
      <div className="bg-components-panel-bg shadow rounded-lg border border-divider-subtle">
        <div className="px-4 py-5 sm:p-6">
          <h3 className="text-lg leading-6 font-medium text-text-primary mb-4">
            关键设备
          </h3>
          <div className="space-y-3">
            {[...Array(6)].map((_, i) => (
              <div key={i} className="animate-pulse">
                <div className="flex items-center space-x-3 p-3 rounded-lg">
                  <div className="w-8 h-8 bg-components-panel-bg-alt rounded-lg"></div>
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
            关键设备
          </h3>
          <button className="text-sm text-components-button-primary-bg hover:text-components-button-primary-bg-hover">
            查看全部
          </button>
        </div>
        
        {!devices || devices.length === 0 ? (
          <div className="text-center py-8 text-text-tertiary">
            <RiDeviceLine className="w-12 h-12 mx-auto mb-2" />
            <div className="text-sm">暂无设备数据</div>
          </div>
        ) : (
          <div className="space-y-1">
            {devices.slice(0, 8).map((device) => (
              <div 
                key={device.id} 
                className="flex items-center space-x-3 p-3 rounded-lg hover:bg-components-panel-bg-alt transition-colors cursor-pointer"
              >
                <div className="flex-shrink-0">
                  <div className="w-8 h-8 bg-components-panel-bg-alt rounded-lg flex items-center justify-center border border-divider-subtle">
                    {getStatusIcon(device.status)}
                  </div>
                </div>
                <div className="flex-1 min-w-0">
                  <div className="flex items-center space-x-2 mb-1">
                    <p className="text-sm font-medium text-text-primary truncate">
                      {device.name}
                    </p>
                    <span className={cn(
                      "inline-flex items-center px-2 py-0.5 rounded text-xs font-medium",
                      getStatusColor(device.status)
                    )}>
                      {getStatusText(device.status)}
                    </span>
                  </div>
                  <div className="flex items-center space-x-2 text-xs text-text-tertiary">
                    <span>{device.type}</span>
                    <span>•</span>
                    <span>最后在线: {formatTime(device.lastSeen)}</span>
                  </div>
                </div>
              </div>
            ))}
          </div>
        )}
      </div>
    </div>
  )
}

export default QuickDevices