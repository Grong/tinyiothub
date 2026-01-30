'use client'

import React from 'react'
import { RiCpuLine, RiHardDrive2Line, RiRamLine, RiWifiLine } from '@remixicon/react'
import type { DashboardMetrics } from '@/types'
import cn from '@/utils/classnames'

interface SystemMetricsProps {
  metrics: DashboardMetrics | null
  loading?: boolean
}

const SystemMetrics = ({ metrics, loading }: SystemMetricsProps) => {
  const getUsageColor = (percentage: number) => {
    if (percentage >= 90) return 'bg-text-destructive'
    if (percentage >= 70) return 'bg-text-warning'
    return 'bg-text-success'
  }

  const formatBytes = (bytes: number) => {
    if (bytes === 0) return '0 B'
    const k = 1024
    const sizes = ['B', 'KB', 'MB', 'GB', 'TB']
    const i = Math.floor(Math.log(bytes) / Math.log(k))
    return parseFloat((bytes / Math.pow(k, i)).toFixed(2)) + ' ' + sizes[i]
  }

  if (loading) {
    return (
      <div className="bg-components-panel-bg shadow rounded-lg border border-divider-subtle">
        <div className="px-4 py-5 sm:p-6">
          <h3 className="text-lg leading-6 font-medium text-text-primary mb-4">
            系统性能
          </h3>
          <div className="space-y-6 animate-pulse">
            {[...Array(4)].map((_, i) => (
              <div key={i} className="space-y-2">
                <div className="flex items-center justify-between">
                  <div className="flex items-center space-x-2">
                    <div className="w-4 h-4 bg-components-panel-bg-alt rounded"></div>
                    <div className="h-4 bg-components-panel-bg-alt rounded w-16"></div>
                  </div>
                  <div className="h-4 bg-components-panel-bg-alt rounded w-12"></div>
                </div>
                <div className="w-full bg-components-panel-bg-alt rounded-full h-2"></div>
              </div>
            ))}
          </div>
        </div>
      </div>
    )
  }

  const metricsData = [
    {
      name: 'CPU 使用率',
      icon: RiCpuLine,
      value: metrics?.cpu || 0,
      unit: '%',
      description: '处理器使用情况'
    },
    {
      name: '内存使用率',
      icon: RiRamLine,
      value: metrics?.memory || 0,
      unit: '%',
      description: '内存占用情况'
    },
    {
      name: '磁盘使用率',
      icon: RiHardDrive2Line,
      value: metrics?.disk || 0,
      unit: '%',
      description: '存储空间使用'
    },
    {
      name: '网络流量',
      icon: RiWifiLine,
      value: 0, // 这里可以显示网络使用率或者流量
      unit: '',
      description: `↑${formatBytes(metrics?.network?.outbound || 0)} ↓${formatBytes(metrics?.network?.inbound || 0)}`,
      isNetwork: true
    }
  ]

  return (
    <div className="bg-components-panel-bg shadow rounded-lg border border-divider-subtle">
      <div className="px-4 py-5 sm:p-6">
        <h3 className="text-lg leading-6 font-medium text-text-primary mb-4">
          系统性能
        </h3>
        
        <div className="space-y-6">
          {metricsData.map((metric, index) => (
            <div key={index} className="space-y-2">
              <div className="flex items-center justify-between">
                <div className="flex items-center space-x-2">
                  <metric.icon className="w-4 h-4 text-text-secondary" />
                  <span className="text-sm font-medium text-text-primary">
                    {metric.name}
                  </span>
                </div>
                {!metric.isNetwork && (
                  <span className="text-sm font-medium text-text-primary">
                    {metric.value.toFixed(1)}{metric.unit}
                  </span>
                )}
              </div>
              
              {metric.isNetwork ? (
                <div className="text-xs text-text-tertiary">
                  {metric.description}
                </div>
              ) : (
                <>
                  <div className="w-full bg-components-panel-bg-alt rounded-full h-2">
                    <div
                      className={cn(
                        "h-2 rounded-full transition-all duration-300",
                        getUsageColor(metric.value)
                      )}
                      style={{ width: `${Math.min(metric.value, 100)}%` }}
                    ></div>
                  </div>
                  <div className="text-xs text-text-tertiary">
                    {metric.description}
                  </div>
                </>
              )}
            </div>
          ))}
        </div>
      </div>
    </div>
  )
}

export default SystemMetrics