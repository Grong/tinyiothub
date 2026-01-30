'use client'

import React, { useState } from 'react'
import { 
  RiFileTextLine,
  RiErrorWarningLine,
  RiAlarmWarningLine,
  RiInformationLine,
  RiTimeLine,
  RiFilterLine,
  RiRefreshLine,
  RiDeleteBinLine,
  RiEyeLine,
  RiCodeLine
} from '@remixicon/react'
import { useDeviceTraces, useClearDeviceTraces } from '@/service/device-monitoring'
import type { DeviceTrace, TraceQuery } from '@/service/device-monitoring'
import cn from '@/utils/classnames'

interface DeviceTraceRecordsProps {
  deviceId: string
  refreshKey?: number
}

const DeviceTraceRecords = ({ deviceId, refreshKey }: DeviceTraceRecordsProps) => {
  const [filters, setFilters] = useState<TraceQuery>({
    limit: 50,
    offset: 0
  })
  const [selectedTrace, setSelectedTrace] = useState<DeviceTrace | null>(null)
  const [showFilters, setShowFilters] = useState(false)

  const { data: traces, isLoading, refetch } = useDeviceTraces(deviceId, filters)
  const clearTracesMutation = useClearDeviceTraces()

  const traceTypeOptions = [
    { value: 'operation', label: '操作' },
    { value: 'status_change', label: '状态变更' },
    { value: 'error', label: '错误' },
    { value: 'warning', label: '警告' },
    { value: 'info', label: '信息' }
  ]

  const levelOptions = [
    { value: 'debug', label: '调试' },
    { value: 'info', label: '信息' },
    { value: 'warn', label: '警告' },
    { value: 'error', label: '错误' },
    { value: 'critical', label: '严重' }
  ]

  const getLevelIcon = (level: string) => {
    switch (level) {
      case 'error':
      case 'critical':
        return RiErrorWarningLine
      case 'warn':
        return RiAlarmWarningLine
      default:
        return RiInformationLine
    }
  }

  const getLevelColor = (level: string) => {
    switch (level) {
      case 'critical':
        return 'text-text-destructive bg-red-50 border-red-200'
      case 'error':
        return 'text-text-destructive bg-red-50 border-red-200'
      case 'warn':
        return 'text-text-warning bg-yellow-50 border-yellow-200'
      case 'info':
        return 'text-text-success bg-green-50 border-green-200'
      case 'debug':
        return 'text-text-secondary bg-gray-50 border-gray-200'
      default:
        return 'text-text-secondary bg-gray-50 border-gray-200'
    }
  }

  const getLevelBadgeColor = (level: string) => {
    switch (level) {
      case 'critical':
        return 'bg-text-destructive text-white'
      case 'error':
        return 'bg-text-destructive text-white'
      case 'warn':
        return 'bg-text-warning text-white'
      case 'info':
        return 'bg-text-success text-white'
      case 'debug':
        return 'bg-text-secondary text-white'
      default:
        return 'bg-text-secondary text-white'
    }
  }

  const formatTimestamp = (timestamp: string) => {
    try {
      const date = new Date(timestamp)
      return date.toLocaleString('zh-CN', {
        year: 'numeric',
        month: '2-digit',
        day: '2-digit',
        hour: '2-digit',
        minute: '2-digit',
        second: '2-digit'
      })
    } catch {
      return timestamp
    }
  }

  const handleFilterChange = (key: keyof TraceQuery, value: any) => {
    setFilters(prev => ({
      ...prev,
      [key]: value,
      offset: 0 // 重置分页
    }))
  }

  const handleClearFilters = () => {
    setFilters({
      limit: 50,
      offset: 0
    })
  }

  const handleClearTraces = async () => {
    if (!confirm('确定要清理追踪记录吗？此操作不可撤销。')) return
    
    try {
      await clearTracesMutation.mutateAsync({
        deviceId,
        data: {
          beforeDate: new Date(Date.now() - 7 * 24 * 60 * 60 * 1000).toISOString() // 清理7天前的记录
        }
      })
      refetch()
    } catch (error) {
      console.error('清理追踪记录失败:', error)
    }
  }

  const handleLoadMore = () => {
    setFilters(prev => ({
      ...prev,
      offset: (prev.offset || 0) + (prev.limit || 50)
    }))
  }

  React.useEffect(() => {
    if (refreshKey) {
      refetch()
    }
  }, [refreshKey, refetch])

  React.useEffect(() => {
    refetch()
  }, [filters, refetch])

  if (isLoading && !traces) {
    return (
      <div className="bg-components-panel-bg shadow rounded-lg border border-divider-subtle">
        <div className="px-4 py-5 sm:p-6">
          <h3 className="text-lg leading-6 font-medium text-text-primary mb-4">
            追踪记录
          </h3>
          <div className="space-y-4 animate-pulse">
            {[...Array(5)].map((_, i) => (
              <div key={i} className="border border-divider-subtle rounded-lg p-4">
                <div className="flex items-start space-x-3">
                  <div className="w-8 h-8 bg-components-panel-bg-alt rounded-full"></div>
                  <div className="flex-1 space-y-2">
                    <div className="flex items-center justify-between">
                      <div className="h-4 bg-components-panel-bg-alt rounded w-32"></div>
                      <div className="h-3 bg-components-panel-bg-alt rounded w-24"></div>
                    </div>
                    <div className="h-3 bg-components-panel-bg-alt rounded w-full"></div>
                    <div className="h-3 bg-components-panel-bg-alt rounded w-3/4"></div>
                  </div>
                </div>
              </div>
            ))}
          </div>
        </div>
      </div>
    )
  }

  return (
    <div className="space-y-6">
      {/* 过滤器和操作 */}
      <div className="bg-components-panel-bg shadow rounded-lg border border-divider-subtle">
        <div className="px-4 py-3 border-b border-divider-subtle">
          <div className="flex items-center justify-between">
            <h3 className="text-lg leading-6 font-medium text-text-primary">
              追踪记录
            </h3>
            <div className="flex items-center space-x-2">
              <button
                onClick={() => setShowFilters(!showFilters)}
                className={cn(
                  "inline-flex items-center px-3 py-2 border border-divider-subtle rounded-md shadow-sm text-sm font-medium bg-components-panel-bg hover:bg-components-panel-bg-alt focus:outline-none focus:ring-2 focus:ring-offset-2 focus:ring-components-button-primary-bg",
                  showFilters ? "text-components-button-primary-bg" : "text-text-secondary"
                )}
              >
                <RiFilterLine className="w-4 h-4 mr-2" />
                过滤器
              </button>
              <button
                onClick={() => refetch()}
                className="inline-flex items-center px-3 py-2 border border-divider-subtle rounded-md shadow-sm text-sm font-medium text-text-secondary bg-components-panel-bg hover:bg-components-panel-bg-alt focus:outline-none focus:ring-2 focus:ring-offset-2 focus:ring-components-button-primary-bg"
              >
                <RiRefreshLine className="w-4 h-4 mr-2" />
                刷新
              </button>
              <button
                onClick={handleClearTraces}
                disabled={clearTracesMutation.isPending}
                className="inline-flex items-center px-3 py-2 border border-divider-subtle rounded-md shadow-sm text-sm font-medium text-text-destructive bg-components-panel-bg hover:bg-red-50 focus:outline-none focus:ring-2 focus:ring-offset-2 focus:ring-red-500 disabled:opacity-50"
              >
                <RiDeleteBinLine className="w-4 h-4 mr-2" />
                清理记录
              </button>
            </div>
          </div>

          {/* 过滤器面板 */}
          {showFilters && (
            <div className="mt-4 pt-4 border-t border-divider-subtle">
              <div className="grid grid-cols-1 md:grid-cols-3 gap-4">
                <div>
                  <label className="block text-sm font-medium text-text-primary mb-2">
                    追踪类型
                  </label>
                  <select
                    multiple
                    value={filters.traceTypes || []}
                    onChange={(e) => {
                      const values = Array.from(e.target.selectedOptions, option => option.value)
                      handleFilterChange('traceTypes', values.length > 0 ? values : undefined)
                    }}
                    className="block w-full pl-3 pr-10 py-2 text-base border border-divider-subtle focus:outline-none focus:ring-components-button-primary-bg focus:border-components-button-primary-bg sm:text-sm rounded-md bg-components-panel-bg"
                  >
                    {traceTypeOptions.map((option) => (
                      <option key={option.value} value={option.value}>
                        {option.label}
                      </option>
                    ))}
                  </select>
                </div>

                <div>
                  <label className="block text-sm font-medium text-text-primary mb-2">
                    级别
                  </label>
                  <select
                    multiple
                    value={filters.levels || []}
                    onChange={(e) => {
                      const values = Array.from(e.target.selectedOptions, option => option.value)
                      handleFilterChange('levels', values.length > 0 ? values : undefined)
                    }}
                    className="block w-full pl-3 pr-10 py-2 text-base border border-divider-subtle focus:outline-none focus:ring-components-button-primary-bg focus:border-components-button-primary-bg sm:text-sm rounded-md bg-components-panel-bg"
                  >
                    {levelOptions.map((option) => (
                      <option key={option.value} value={option.value}>
                        {option.label}
                      </option>
                    ))}
                  </select>
                </div>

                <div>
                  <label className="block text-sm font-medium text-text-primary mb-2">
                    每页显示
                  </label>
                  <select
                    value={filters.limit || 50}
                    onChange={(e) => handleFilterChange('limit', Number(e.target.value))}
                    className="block w-full pl-3 pr-10 py-2 text-base border border-divider-subtle focus:outline-none focus:ring-components-button-primary-bg focus:border-components-button-primary-bg sm:text-sm rounded-md bg-components-panel-bg"
                  >
                    <option value={20}>20</option>
                    <option value={50}>50</option>
                    <option value={100}>100</option>
                  </select>
                </div>
              </div>

              <div className="mt-4 flex items-center justify-between">
                <div className="text-sm text-text-tertiary">
                  {filters.traceTypes || filters.levels ? '已应用过滤器' : '显示所有记录'}
                </div>
                <button
                  onClick={handleClearFilters}
                  className="text-sm text-components-button-primary-bg hover:text-components-button-primary-bg-hover"
                >
                  清除过滤器
                </button>
              </div>
            </div>
          )}
        </div>

        {/* 记录列表 */}
        <div className="divide-y divide-divider-subtle">
          {!traces || traces.length === 0 ? (
            <div className="text-center py-12">
              <RiFileTextLine className="w-12 h-12 mx-auto mb-4 text-text-tertiary" />
              <div className="text-sm text-text-tertiary">暂无追踪记录</div>
              <div className="text-xs text-text-tertiary mt-1">
                {filters.traceTypes || filters.levels ? '尝试调整过滤条件' : '设备操作记录将在此显示'}
              </div>
            </div>
          ) : (
            <>
              {traces.map((trace, index) => {
                const LevelIcon = getLevelIcon(trace.level)
                
                return (
                  <div key={trace.id} className="p-4 hover:bg-components-panel-bg-alt">
                    <div className="flex items-start space-x-3">
                      <div className={cn(
                        "w-8 h-8 rounded-full flex items-center justify-center flex-shrink-0",
                        getLevelBadgeColor(trace.level)
                      )}>
                        <LevelIcon className="w-4 h-4" />
                      </div>
                      
                      <div className="flex-1 min-w-0">
                        <div className="flex items-center justify-between mb-2">
                          <div className="flex items-center space-x-2">
                            <h4 className="text-sm font-medium text-text-primary truncate">
                              {trace.title}
                            </h4>
                            <span className={cn(
                              "inline-flex items-center px-2 py-1 rounded-full text-xs font-medium",
                              getLevelBadgeColor(trace.level)
                            )}>
                              {trace.level}
                            </span>
                            <span className="inline-flex items-center px-2 py-1 rounded-full text-xs font-medium bg-components-panel-bg-alt text-text-secondary">
                              {trace.traceType}
                            </span>
                          </div>
                          <div className="flex items-center space-x-2">
                            {trace.details && (
                              <button
                                onClick={() => setSelectedTrace(trace)}
                                className="text-xs text-components-button-primary-bg hover:text-components-button-primary-bg-hover"
                              >
                                <RiEyeLine className="w-4 h-4" />
                              </button>
                            )}
                            <div className="flex items-center text-xs text-text-tertiary">
                              <RiTimeLine className="w-3 h-3 mr-1" />
                              {formatTimestamp(trace.createdAt)}
                            </div>
                          </div>
                        </div>
                        
                        <p className="text-sm text-text-secondary mb-2">
                          {trace.message}
                        </p>
                        
                        <div className="flex items-center justify-between text-xs text-text-tertiary">
                          <div className="flex items-center space-x-4">
                            <span>分类: {trace.category}</span>
                            {trace.source && <span>来源: {trace.source}</span>}
                            {trace.userId && <span>用户: {trace.userId}</span>}
                          </div>
                        </div>
                      </div>
                    </div>
                  </div>
                )
              })}

              {/* 加载更多 */}
              <div className="p-4 text-center border-t border-divider-subtle">
                <button
                  onClick={handleLoadMore}
                  disabled={isLoading}
                  className="inline-flex items-center px-4 py-2 border border-divider-subtle rounded-md shadow-sm text-sm font-medium text-text-secondary bg-components-panel-bg hover:bg-components-panel-bg-alt focus:outline-none focus:ring-2 focus:ring-offset-2 focus:ring-components-button-primary-bg disabled:opacity-50"
                >
                  {isLoading ? '加载中...' : '加载更多'}
                </button>
              </div>
            </>
          )}
        </div>
      </div>

      {/* 详情模态框 */}
      {selectedTrace && (
        <div className="fixed inset-0 bg-black bg-opacity-50 flex items-center justify-center p-4 z-50">
          <div className="bg-components-panel-bg rounded-lg shadow-xl max-w-2xl w-full max-h-[80vh] overflow-hidden">
            <div className="px-6 py-4 border-b border-divider-subtle">
              <div className="flex items-center justify-between">
                <h3 className="text-lg font-medium text-text-primary">
                  追踪记录详情
                </h3>
                <button
                  onClick={() => setSelectedTrace(null)}
                  className="text-text-tertiary hover:text-text-secondary"
                >
                  ✕
                </button>
              </div>
            </div>
            
            <div className="px-6 py-4 overflow-y-auto">
              <div className="space-y-4">
                <div>
                  <label className="block text-sm font-medium text-text-primary mb-1">
                    标题
                  </label>
                  <div className="text-sm text-text-secondary">
                    {selectedTrace.title}
                  </div>
                </div>
                
                <div>
                  <label className="block text-sm font-medium text-text-primary mb-1">
                    消息
                  </label>
                  <div className="text-sm text-text-secondary">
                    {selectedTrace.message}
                  </div>
                </div>
                
                {selectedTrace.details && (
                  <div>
                    <label className="block text-sm font-medium text-text-primary mb-1">
                      详细信息
                    </label>
                    <div className="bg-components-panel-bg-alt rounded-md p-3">
                      <pre className="text-xs text-text-secondary whitespace-pre-wrap overflow-x-auto">
                        {JSON.stringify(JSON.parse(selectedTrace.details), null, 2)}
                      </pre>
                    </div>
                  </div>
                )}
                
                <div className="grid grid-cols-2 gap-4">
                  <div>
                    <label className="block text-sm font-medium text-text-primary mb-1">
                      级别
                    </label>
                    <div className="text-sm text-text-secondary">
                      {selectedTrace.level}
                    </div>
                  </div>
                  
                  <div>
                    <label className="block text-sm font-medium text-text-primary mb-1">
                      类型
                    </label>
                    <div className="text-sm text-text-secondary">
                      {selectedTrace.traceType}
                    </div>
                  </div>
                  
                  <div>
                    <label className="block text-sm font-medium text-text-primary mb-1">
                      分类
                    </label>
                    <div className="text-sm text-text-secondary">
                      {selectedTrace.category}
                    </div>
                  </div>
                  
                  <div>
                    <label className="block text-sm font-medium text-text-primary mb-1">
                      来源
                    </label>
                    <div className="text-sm text-text-secondary">
                      {selectedTrace.source || '--'}
                    </div>
                  </div>
                  
                  <div>
                    <label className="block text-sm font-medium text-text-primary mb-1">
                      用户ID
                    </label>
                    <div className="text-sm text-text-secondary">
                      {selectedTrace.userId || '--'}
                    </div>
                  </div>
                  
                  <div>
                    <label className="block text-sm font-medium text-text-primary mb-1">
                      创建时间
                    </label>
                    <div className="text-sm text-text-secondary">
                      {formatTimestamp(selectedTrace.createdAt)}
                    </div>
                  </div>
                </div>
              </div>
            </div>
          </div>
        </div>
      )}
    </div>
  )
}

export default DeviceTraceRecords