'use client'

import React from 'react'
import { 
  RiCpuLine, 
  RiRamLine, 
  RiWifiLine, 
  RiTimeLine,
  RiSpeedLine,
  RiErrorWarningLine,
  RiHeartPulseLine
} from '@remixicon/react'
import type { DevicePerformanceMetrics } from '@/service/device-monitoring'
import cn from '@/utils/classnames'

interface DevicePerformanceMetricsProps {
  performanceMetrics?: DevicePerformanceMetrics | null
  loading?: boolean
  compact?: boolean
}

const DevicePerformanceMetrics = ({ performanceMetrics, loading, compact = false }: DevicePerformanceMetricsProps) => {
  const getUsageColor = (percentage?: number) => {
    if (!percentage) return 'text-text-tertiary'
    if (percentage >= 90) return 'text-text-destructive'
    if (percentage >= 70) return 'text-text-warning'
    return 'text-text-success'
  }

  const getProgressBarColor = (percentage?: number) => {
    if (!percentage) return 'bg-components-panel-bg-alt'
    if (percentage >= 90) return 'bg-text-destructive'
    if (percentage >= 70) return 'bg-text-warning'
    return 'bg-text-success'
  }

  const formatValue = (value?: number, unit: string = '', decimals: number = 1) => {
    if (value === undefined || value === null) return '--'
    return `${value.toFixed(decimals)}${unit}`
  }

  const formatLastUpdated = (lastUpdated?: string) => {
    if (!lastUpdated) return '--'
    try {
      const date = new Date(lastUpdated)
      const now = new Date()
      const diffMs = now.getTime() - date.getTime()
      const diffMinutes = Math.floor(diffMs / (1000 * 60))
      
      if (diffMinutes < 1) return '刚刚'
      if (diffMinutes < 60) return `${diffMinutes}分钟前`
      return `${Math.floor(diffMinutes / 60)}小时前`
    } catch {
      return lastUpdated
    }
  }

  const metricsData = [
    {
      name: 'CPU 使用率',
      icon: RiCpuLine,
      value: performanceMetrics?.cpuUsage,
      unit: '%',
      showProgress: true,
      description: '处理器使用情况'
    },
    {
      name: '内存使用率',
      icon: RiRamLine,
      value: performanceMetrics?.memoryUsage,
      unit: '%',
      showProgress: true,
      description: '内存占用情况'
    },
    {
      name: '网络延迟',
      icon: RiWifiLine,
      value: performanceMetrics?.networkLatencyMs,
      unit: 'ms',
      showProgress: false,
      description: '网络通信延迟'
    },
    {
      name: '响应时间',
      icon: RiTimeLine,
      value: performanceMetrics?.responseTimeMs,
      unit: 'ms',
      showProgress: false,
      description: '设备响应时间'
    },
    {
      name: '吞吐量',
      icon: RiSpeedLine,
      value: performanceMetrics?.throughputOpsPerSec,
      unit: ' ops/s',
      showProgress: false,
      description: '每秒操作数'
    },
    {
      name: '错误率',
      icon: RiErrorWarningLine,
      value: performanceMetrics?.errorRate ? performanceMetrics.errorRate * 100 : undefined,
      unit: '%',
      showProgress: true,
      description: '操作错误率'
    },
    {
      name: '正常运行时间',
      icon: RiHeartPulseLine,
      value: performanceMetrics?.uptimePercentage,
      unit: '%',
      showProgress: true,
      description: '设备正常运行时间'
    }
  ]

  if (loading) {
    return (
      <div className="bg-components-panel-bg shadow rounded-lg border border-divider-subtle">
        <div className="px-4 py-5 sm:p-6">
          <h3 className="text-lg leading-6 font-medium text-text-primary mb-4">
            性能指标
          </h3>
          <div className={cn(
            "space-y-6 animate-pulse",
            compact ? "grid grid-cols-2 gap-4 space-y-0" : ""
          )}>
            {[...Array(compact ? 4 : 7)].map((_, i) => (
              <div key={i} className="space-y-2">
                <div className="flex items-center justify-between">
                  <div className="flex items-center space-x-2">
                    <div className="w-4 h-4 bg-components-panel-bg-alt rounded"></div>
                    <div className="h-4 bg-components-panel-bg-alt rounded w-20"></div>
                  </div>
                  <div className="h-4 bg-components-panel-bg-alt rounded w-12"></div>
                </div>
                {!compact && (
                  <div className="w-full bg-components-panel-bg-alt rounded-full h-2"></div>
                )}
              </div>
            ))}
          </div>
        </div>
      </div>
    )
  }

  const displayMetrics = compact ? metricsData.slice(0, 4) : metricsData

  return (
    <div className="bg-components-panel-bg shadow rounded-lg border border-divider-subtle">
      <div className="px-4 py-5 sm:p-6">
        <div className="flex items-center justify-between mb-4">
          <h3 className="text-lg leading-6 font-medium text-text-primary">
            性能指标
          </h3>
          <div className="text-xs text-text-tertiary">
            更新时间: {formatLastUpdated(performanceMetrics?.lastUpdated)}
          </div>
        </div>
        
        <div className={cn(
          "space-y-6",
          compact ? "grid grid-cols-2 gap-4 space-y-0" : ""
        )}>
          {displayMetrics.map((metric, index) => (
            <div key={index} className="space-y-2">
              <div className="flex items-center justify-between">
                <div className="flex items-center space-x-2">
                  <metric.icon className="w-4 h-4 text-text-secondary" />
                  <span className="text-sm font-medium text-text-primary">
                    {metric.name}
                  </span>
                </div>
                <span className={cn(
                  "text-sm font-medium",
                  getUsageColor(metric.value)
                )}>
                  {formatValue(metric.value, metric.unit)}
                </span>
              </div>
              
              {metric.showProgress && !compact && metric.value !== undefined && (
                <div className="w-full bg-components-panel-bg-alt rounded-full h-2">
                  <div
                    className={cn(
                      "h-2 rounded-full transition-all duration-300",
                      getProgressBarColor(metric.value)
                    )}
                    style={{ width: `${Math.min(metric.value, 100)}%` }}
                  ></div>
                </div>
              )}
              
              {!compact && (
                <div className="text-xs text-text-tertiary">
                  {metric.description}
                </div>
              )}
            </div>
          ))}
        </div>

        {compact && (
          <div className="mt-4 pt-4 border-t border-divider-subtle">
            <div className="text-center">
              <button className="text-sm text-components-button-primary-bg hover:text-components-button-primary-bg-hover">
                查看详细性能数据 →
              </button>
            </div>
          </div>
        )}
      </div>
    </div>
  )
}

export default DevicePerformanceMetrics