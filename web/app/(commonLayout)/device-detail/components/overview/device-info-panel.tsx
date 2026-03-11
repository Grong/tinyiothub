'use client'

import React from 'react'
import { RiWifiLine, RiWifiOffLine, RiAlarmWarningLine, RiSettings3Line } from '@remixicon/react'
import { useDevice } from '@/service/devices'
import cn from '@/utils/classnames'

type DeviceInfoPanelProps = {
  deviceId: string
}

const DeviceInfoPanel = ({ deviceId }: DeviceInfoPanelProps) => {
  const { data: device, isLoading } = useDevice(deviceId)

  if (isLoading || !device) {
    return (
      <div className="mb-6 rounded-xl bg-components-panel-bg p-6 shadow-sm">
        <div className="animate-pulse">
          <div className="h-6 bg-components-panel-bg-alt rounded mb-4"></div>
          <div className="grid grid-cols-1 md:grid-cols-3 gap-4">
            <div className="h-20 bg-components-panel-bg-alt rounded"></div>
            <div className="h-20 bg-components-panel-bg-alt rounded"></div>
            <div className="h-20 bg-components-panel-bg-alt rounded"></div>
          </div>
        </div>
      </div>
    )
  }

  const getStatusIcon = (status?: string) => {
    switch (status) {
      case 'online':
        return <RiWifiLine className='h-5 w-5 text-text-success' />
      case 'offline':
        return <RiWifiOffLine className='h-5 w-5 text-text-tertiary' />
      case 'error':
        return <RiAlarmWarningLine className='h-5 w-5 text-text-destructive' />
      case 'maintenance':
        return <RiSettings3Line className='h-5 w-5 text-text-warning' />
      default:
        return <RiWifiOffLine className='h-5 w-5 text-text-tertiary' />
    }
  }

  const getStatusColor = (status?: string) => {
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

  const getStatusText = (status?: string) => {
    switch (status) {
      case 'online':
        return '在线'
      case 'offline':
        return '离线'
      case 'error':
        return '故障'
      case 'maintenance':
        return '维护中'
      default:
        return '未知'
    }
  }

  // 格式化最后在线时间
  const formatLastSeen = (timestamp?: string) => {
    if (!timestamp) return '从未上线'
    
    try {
      const date = new Date(timestamp)
      return date.toLocaleString('zh-CN', {
        year: 'numeric',
        month: '2-digit',
        day: '2-digit',
        hour: '2-digit',
        minute: '2-digit',
      })
    } catch {
      return '时间格式错误'
    }
  }

  return (
    <div className="mb-6 rounded-xl bg-components-panel-bg p-6 shadow-sm border border-divider-subtle">
      <div className="flex items-center justify-between mb-6">
        <div className="flex items-center gap-4">
          <div className="flex h-12 w-12 items-center justify-center rounded-lg bg-components-panel-bg-alt border border-divider-subtle">
            {getStatusIcon(device.status)}
          </div>
          <div>
            <h1 className="text-xl font-semibold text-text-primary">{device.name}</h1>
            <p className="text-sm text-text-secondary">{device.description || '暂无描述'}</p>
          </div>
        </div>
        <span className={cn(
          'inline-flex items-center px-3 py-1 rounded-full text-sm font-medium',
          getStatusColor(device.status)
        )}>
          {getStatusText(device.status)}
        </span>
      </div>

      <div className="grid grid-cols-1 md:grid-cols-3 gap-4">
        <div className="bg-components-panel-bg-alt rounded-lg p-4 border border-divider-subtle">
          <div className="text-sm text-text-tertiary mb-1">产品名称</div>
          <div className="text-base font-medium text-text-primary">{device.productName || '未知产品'}</div>
        </div>
        
        <div className="bg-components-panel-bg-alt rounded-lg p-4 border border-divider-subtle">
          <div className="text-sm text-text-tertiary mb-1">最后在线</div>
          <div className="text-base font-medium text-text-primary">{formatLastSeen(device.updatedAt)}</div>
        </div>
        
        <div className="bg-components-panel-bg-alt rounded-lg p-4 border border-divider-subtle">
          <div className="text-sm text-text-tertiary mb-1">设备ID</div>
          <div className="text-base font-medium text-text-primary font-mono">{device.id}</div>
        </div>
      </div>

      {device.properties && device.properties.length > 0 && (
        <div className="mt-6">
          <h3 className="text-sm font-medium text-text-secondary mb-3">设备属性</h3>
          <div className="grid grid-cols-1 md:grid-cols-2 lg:grid-cols-3 gap-3">
            {device.properties.map((prop: any, index: number) => (
              <div key={index} className="bg-components-panel-bg-alt rounded-lg p-3 border border-divider-subtle">
                <div className="text-xs text-text-tertiary mb-1">{prop.name}</div>
                <div className="text-sm font-medium text-text-primary">{prop.value} {prop.unit && <span className="text-text-tertiary">{prop.unit}</span>}</div>
              </div>
            ))}
          </div>
        </div>
      )}
    </div>
  )
}

export default DeviceInfoPanel