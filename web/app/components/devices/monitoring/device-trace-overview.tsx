'use client'

import React from 'react'
import { 
  RiFileTextLine,
  RiErrorWarningLine,
  RiAlarmWarningLine,
  RiInformationLine,
  RiTimeLine,
  RiBarChartLine
} from '@remixicon/react'
import type { DeviceTraceStatistics } from '@/service/device-monitoring'
import cn from '@/utils/classnames'

interface DeviceTraceOverviewProps {
  traceStats?: DeviceTraceStatistics | null
  loading?: boolean
}

const DeviceTraceOverview = ({ traceStats, loading }: DeviceTraceOverviewProps) => {
  const formatLastTraceTime = (lastTraceTime?: string) => {
    if (!lastTraceTime) return '--'
    try {
      const date = new Date(lastTraceTime)
      const now = new Date()
      const diffMs = now.getTime() - date.getTime()
      const diffMinutes = Math.floor(diffMs / (1000 * 60))
      
      if (diffMinutes < 1) return '刚刚'
      if (diffMinutes < 60) return `${diffMinutes}分钟前`
      if (diffMinutes < 1440) return `${Math.floor(diffMinutes / 60)}小时前`
      return `${Math.floor(diffMinutes / 1440)}天前`
    } catch {
      return lastTraceTime
    }
  }

  const getTraceDistribution = () => {
    if (!traceStats) return []
    
    const total = traceStats.totalTraces
    if (total === 0) return []
    
    return [
      {
        name: '错误',
        count: traceStats.errorTraces,
        percentage: ((traceStats.errorTraces / total) * 100).toFixed(1),
        color: 'bg-text-destructive',
        icon: RiErrorWarningLine
      },
      {
        name: '警告',
        count: traceStats.warningTraces,
        percentage: ((traceStats.warningTraces / total) * 100).toFixed(1),
        color: 'bg-text-warning',
        icon: RiAlarmWarningLine
      },
      {
        name: '信息',
        count: traceStats.infoTraces,
        percentage: ((traceStats.infoTraces / total) * 100).toFixed(1),
        color: 'bg-text-success',
        icon: RiInformationLine
      }
    ]
  }

  if (loading) {
    return (
      <div className="bg-components-panel-bg shadow rounded-lg border border-divider-subtle">
        <div className="px-4 py-5 sm:p-6">
          <h3 className="text-lg leading-6 font-medium text-text-primary mb-4">
            追踪记录概览
          </h3>
          <div className="animate-pulse">
            <div className="grid grid-cols-1 md:grid-cols-2 gap-6">
              <div className="space-y-4">
                <div className="flex items-center justify-between">
                  <div className="h-4 bg-components-panel-bg-alt rounded w-20"></div>
                  <div className="h-6 bg-components-panel-bg-alt rounded w-12"></div>
                </div>
                <div className="space-y-3">
                  {[...Array(3)].map((_, i) => (
                    <div key={i} className="flex items-center justify-between">
                      <div className="flex items-center space-x-2">
                        <div className="w-3 h-3 bg-components-panel-bg-alt rounded-full"></div>
                        <div className="h-3 bg-components-panel-bg-alt rounded w-12"></div>
                      </div>
                      <div className="h-3 bg-components-panel-bg-alt rounded w-16"></div>
                    </div>
                  ))}
                </div>
              </div>
              <div className="space-y-4">
                {[...Array(3)].map((_, i) => (
                  <div key={i} className="flex items-center justify-between">
                    <div className="h-3 bg-components-panel-bg-alt rounded w-16"></div>
                    <div className="h-3 bg-components-panel-bg-alt rounded w-20"></div>
                  </div>
                ))}
              </div>
            </div>
          </div>
        </div>
      </div>
    )
  }

  const traceDistribution = getTraceDistribution()

  return (
    <div className="bg-components-panel-bg shadow rounded-lg border border-divider-subtle">
      <div className="px-4 py-5 sm:p-6">
        <div className="flex items-center justify-between mb-4">
          <h3 className="text-lg leading-6 font-medium text-text-primary">
            追踪记录概览
          </h3>
          <div className="text-xs text-text-tertiary">
            统计范围: {traceStats?.daysRange || 7} 天
          </div>
        </div>

        {!traceStats || traceStats.totalTraces === 0 ? (
          <div className="text-center py-8">
            <RiFileTextLine className="w-12 h-12 mx-auto mb-4 text-text-tertiary" />
            <div className="text-sm text-text-tertiary">暂无追踪记录</div>
            <div className="text-xs text-text-tertiary mt-1">
              设备操作记录将在此显示
            </div>
          </div>
        ) : (
          <div className="grid grid-cols-1 md:grid-cols-2 gap-6">
            {/* 总体统计 */}
            <div className="space-y-4">
              <div className="flex items-center justify-between">
                <div className="flex items-center space-x-2">
                  <RiBarChartLine className="w-5 h-5 text-text-secondary" />
                  <span className="text-sm font-medium text-text-primary">
                    记录总数
                  </span>
                </div>
                <span className="text-2xl font-bold text-text-primary">
                  {traceStats.totalTraces.toLocaleString()}
                </span>
              </div>

              {/* 记录分布 */}
              <div className="space-y-3">
                {traceDistribution.map((item, index) => (
                  <div key={index} className="flex items-center justify-between">
                    <div className="flex items-center space-x-2">
                      <div className={cn("w-3 h-3 rounded-full", item.color)}></div>
                      <item.icon className="w-4 h-4 text-text-secondary" />
                      <span className="text-sm text-text-primary">{item.name}</span>
                    </div>
                    <div className="flex items-center space-x-2">
                      <span className="text-sm font-medium text-text-primary">
                        {item.count}
                      </span>
                      <span className="text-xs text-text-tertiary">
                        ({item.percentage}%)
                      </span>
                    </div>
                  </div>
                ))}
              </div>

              {/* 简化的分布条 */}
              <div className="w-full bg-components-panel-bg-alt rounded-full h-2 overflow-hidden">
                <div className="h-full flex">
                  {traceDistribution.map((item, index) => (
                    <div
                      key={index}
                      className={item.color}
                      style={{ width: `${item.percentage}%` }}
                      title={`${item.name}: ${item.count} (${item.percentage}%)`}
                    />
                  ))}
                </div>
              </div>
            </div>

            {/* 详细信息 */}
            <div className="space-y-4">
              <div className="space-y-3">
                <div className="flex items-center justify-between">
                  <span className="text-sm text-text-tertiary">统计时间范围</span>
                  <span className="text-sm font-medium text-text-primary">
                    {traceStats.daysRange} 天
                  </span>
                </div>
                
                <div className="flex items-center justify-between">
                  <span className="text-sm text-text-tertiary">最后记录时间</span>
                  <span className="text-sm font-medium text-text-primary">
                    {formatLastTraceTime(traceStats.lastTraceTime)}
                  </span>
                </div>
                
                <div className="flex items-center justify-between">
                  <span className="text-sm text-text-tertiary">错误率</span>
                  <span className={cn(
                    "text-sm font-medium",
                    traceStats.errorTraces > 0 ? "text-text-destructive" : "text-text-success"
                  )}>
                    {traceStats.totalTraces > 0 
                      ? ((traceStats.errorTraces / traceStats.totalTraces) * 100).toFixed(1)
                      : '0.0'
                    }%
                  </span>
                </div>

                <div className="flex items-center justify-between">
                  <span className="text-sm text-text-tertiary">平均每天记录数</span>
                  <span className="text-sm font-medium text-text-primary">
                    {traceStats.daysRange > 0 
                      ? Math.round(traceStats.totalTraces / traceStats.daysRange)
                      : 0
                    }
                  </span>
                </div>
              </div>

              {/* 状态指示器 */}
              <div className="pt-4 border-t border-divider-subtle">
                <div className="flex items-center space-x-2">
                  <div className={cn(
                    "w-2 h-2 rounded-full",
                    traceStats.errorTraces === 0 ? "bg-text-success" : 
                    traceStats.errorTraces < 5 ? "bg-text-warning" : "bg-text-destructive"
                  )}></div>
                  <span className="text-xs text-text-tertiary">
                    {traceStats.errorTraces === 0 ? '设备运行正常' :
                     traceStats.errorTraces < 5 ? '少量错误记录' : '存在较多错误'}
                  </span>
                </div>
              </div>
            </div>
          </div>
        )}

        {traceStats && traceStats.totalTraces > 0 && (
          <div className="mt-6 pt-4 border-t border-divider-subtle">
            <div className="text-center">
              <button className="text-sm text-components-button-primary-bg hover:text-components-button-primary-bg-hover">
                查看详细追踪记录 →
              </button>
            </div>
          </div>
        )}
      </div>
    </div>
  )
}

export default DeviceTraceOverview