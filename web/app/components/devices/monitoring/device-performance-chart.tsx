'use client'

import React, { useState } from 'react'
import { RiTimeLine, RiRefreshLine } from '@remixicon/react'
import { useDevicePerformanceHistory, type DevicePerformanceMetrics } from '@/service/device-monitoring'
import cn from '@/utils/classnames'

interface DevicePerformanceChartProps {
  deviceId: string
  refreshKey?: number
}

const DevicePerformanceChart = ({ deviceId, refreshKey }: DevicePerformanceChartProps) => {
  const [timeRange, setTimeRange] = useState<number>(24) // 默认24小时
  
  const { data: performanceHistory, isLoading, refetch } = useDevicePerformanceHistory(
    deviceId, 
    timeRange, 
    true
  )

  const timeRangeOptions = [
    { value: 1, label: '1小时' },
    { value: 6, label: '6小时' },
    { value: 24, label: '24小时' },
    { value: 72, label: '3天' },
    { value: 168, label: '7天' }
  ]

  const formatTime = (timestamp: string) => {
    try {
      const date = new Date(timestamp)
      return date.toLocaleTimeString('zh-CN', { 
        hour: '2-digit', 
        minute: '2-digit',
        month: '2-digit',
        day: '2-digit'
      })
    } catch {
      return timestamp
    }
  }

  const getLatestValue = (field: keyof DevicePerformanceMetrics) => {
    if (!performanceHistory || performanceHistory.length === 0) return '--'
    const latest = performanceHistory[performanceHistory.length - 1]
    const value = latest[field as keyof typeof latest]
    if (typeof value === 'number' && value != null) {
      return value.toFixed(1)
    }
    return '--'
  }

  const getValueColor = (value: number, thresholds: { warning: number; critical: number }) => {
    if (value >= thresholds.critical) return 'text-text-destructive'
    if (value >= thresholds.warning) return 'text-text-warning'
    return 'text-text-success'
  }

  // 简化的图表数据点（这里可以后续集成真正的图表库）
  const renderSimpleChart = (data: number[], label: string, unit: string, color: string) => {
    if (!data || data.length === 0) return null
    
    const max = Math.max(...data)
    const min = Math.min(...data)
    const range = max - min || 1
    const latestValue = data[data.length - 1]
    
    return (
      <div className="space-y-2">
        <div className="flex items-center justify-between">
          <span className="text-sm font-medium text-text-primary">{label}</span>
          <span className="text-sm text-text-secondary">
            {latestValue != null ? latestValue.toFixed(1) : '--'}{unit}
          </span>
        </div>
        <div className="h-16 flex items-end space-x-1">
          {data.slice(-20).map((value, index) => {
            if (value == null) return null
            const height = ((value - min) / range) * 100
            return (
              <div
                key={index}
                className={cn("flex-1 rounded-t", color)}
                style={{ height: `${Math.max(height, 2)}%` }}
                title={`${value.toFixed(1)}${unit}`}
              />
            )
          })}
        </div>
      </div>
    )
  }

  React.useEffect(() => {
    if (refreshKey) {
      refetch()
    }
  }, [refreshKey, refetch])

  if (isLoading) {
    return (
      <div className="bg-components-panel-bg shadow rounded-lg border border-divider-subtle">
        <div className="px-4 py-5 sm:p-6">
          <div className="flex items-center justify-between mb-4">
            <h3 className="text-lg leading-6 font-medium text-text-primary">
              性能趋势
            </h3>
            <div className="flex items-center space-x-2">
              <div className="h-8 bg-components-panel-bg-alt rounded w-20 animate-pulse"></div>
              <div className="h-8 bg-components-panel-bg-alt rounded w-8 animate-pulse"></div>
            </div>
          </div>
          <div className="space-y-6 animate-pulse">
            {[...Array(4)].map((_, i) => (
              <div key={i} className="space-y-2">
                <div className="flex items-center justify-between">
                  <div className="h-4 bg-components-panel-bg-alt rounded w-20"></div>
                  <div className="h-4 bg-components-panel-bg-alt rounded w-12"></div>
                </div>
                <div className="h-16 bg-components-panel-bg-alt rounded"></div>
              </div>
            ))}
          </div>
        </div>
      </div>
    )
  }

  const cpuData = performanceHistory?.map(h => h.cpuUsage).filter(v => v != null) as number[] || []
  const memoryData = performanceHistory?.map(h => h.memoryUsage).filter(v => v != null) as number[] || []
  const latencyData = performanceHistory?.map(h => h.networkLatencyMs).filter(v => v != null) as number[] || []
  const responseData = performanceHistory?.map(h => h.responseTimeMs).filter(v => v != null) as number[] || []

  return (
    <div className="bg-components-panel-bg shadow rounded-lg border border-divider-subtle">
      <div className="px-4 py-5 sm:p-6">
        <div className="flex items-center justify-between mb-6">
          <h3 className="text-lg leading-6 font-medium text-text-primary">
            性能趋势
          </h3>
          <div className="flex items-center space-x-2">
            <select
              value={timeRange}
              onChange={(e) => setTimeRange(Number(e.target.value))}
              className="block w-full pl-3 pr-10 py-2 text-base border border-divider-subtle focus:outline-none focus:ring-components-button-primary-bg focus:border-components-button-primary-bg sm:text-sm rounded-md bg-components-panel-bg"
            >
              {timeRangeOptions.map((option) => (
                <option key={option.value} value={option.value}>
                  {option.label}
                </option>
              ))}
            </select>
            <button
              onClick={() => refetch()}
              className="inline-flex items-center p-2 border border-divider-subtle rounded-md shadow-sm text-sm font-medium text-text-secondary bg-components-panel-bg hover:bg-components-panel-bg-alt focus:outline-none focus:ring-2 focus:ring-offset-2 focus:ring-components-button-primary-bg"
            >
              <RiRefreshLine className="w-4 h-4" />
            </button>
          </div>
        </div>

        {!performanceHistory || performanceHistory.length === 0 ? (
          <div className="text-center py-12 text-text-tertiary">
            <RiTimeLine className="w-12 h-12 mx-auto mb-4 text-text-tertiary" />
            <div className="text-sm">暂无性能历史数据</div>
          </div>
        ) : (
          <div className="space-y-8">
            {/* CPU 使用率趋势 */}
            {cpuData.length > 0 && renderSimpleChart(
              cpuData, 
              'CPU 使用率', 
              '%', 
              'bg-blue-500'
            )}

            {/* 内存使用率趋势 */}
            {memoryData.length > 0 && renderSimpleChart(
              memoryData, 
              '内存使用率', 
              '%', 
              'bg-green-500'
            )}

            {/* 网络延迟趋势 */}
            {latencyData.length > 0 && renderSimpleChart(
              latencyData, 
              '网络延迟', 
              'ms', 
              'bg-yellow-500'
            )}

            {/* 响应时间趋势 */}
            {responseData.length > 0 && renderSimpleChart(
              responseData, 
              '响应时间', 
              'ms', 
              'bg-purple-500'
            )}

            {/* 数据点信息 */}
            <div className="mt-6 pt-4 border-t border-divider-subtle">
              <div className="grid grid-cols-2 sm:grid-cols-4 gap-4 text-sm">
                <div>
                  <div className="text-text-tertiary">数据点数</div>
                  <div className="font-medium text-text-primary">
                    {performanceHistory.length}
                  </div>
                </div>
                <div>
                  <div className="text-text-tertiary">时间范围</div>
                  <div className="font-medium text-text-primary">
                    {timeRangeOptions.find(opt => opt.value === timeRange)?.label}
                  </div>
                </div>
                <div>
                  <div className="text-text-tertiary">最新CPU</div>
                  <div className={cn(
                    "font-medium",
                    getValueColor(Number(getLatestValue('cpuUsage')), { warning: 70, critical: 90 })
                  )}>
                    {getLatestValue('cpuUsage')}%
                  </div>
                </div>
                <div>
                  <div className="text-text-tertiary">最新内存</div>
                  <div className={cn(
                    "font-medium",
                    getValueColor(Number(getLatestValue('memoryUsage')), { warning: 80, critical: 95 })
                  )}>
                    {getLatestValue('memoryUsage')}%
                  </div>
                </div>
              </div>
            </div>
          </div>
        )}
      </div>
    </div>
  )
}

export default DevicePerformanceChart