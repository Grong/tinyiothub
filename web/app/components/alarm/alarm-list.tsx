'use client'

import { useState } from 'react'
import {
  RiAlarmWarningLine,
  RiCheckLine,
  RiCloseLine,
  RiFilterLine,
} from '@remixicon/react'
import { useAlarms, useAcknowledgeAlarm, useResolveAlarm, useBatchAcknowledgeAlarms, useBatchResolveAlarms } from '@/service/alarms'
import type { AlarmQueryParams, ResolutionType } from '@/types/alarm'
import Button from '@/app/components/base/button'
import Loading from '@/app/components/base/loading'
import { useToast } from '@/hooks/use-toast'

interface AlarmListProps {
  deviceId?: string
}

const AlarmList: React.FC<AlarmListProps> = ({ deviceId }) => {
  const { toast } = useToast()
  const [selectedIds, setSelectedIds] = useState<string[]>([])
  const [filters, setFilters] = useState<AlarmQueryParams>({
    page: 1,
    pageSize: 20,
    deviceIds: deviceId ? [deviceId] : undefined,
  })

  const { data, isLoading, refetch } = useAlarms(filters)
  const acknowledgeMutation = useAcknowledgeAlarm()
  const resolveMutation = useResolveAlarm()
  const batchAcknowledgeMutation = useBatchAcknowledgeAlarms()
  const batchResolveMutation = useBatchResolveAlarms()

  const handleAcknowledge = async (id: string) => {
    try {
      await acknowledgeMutation.mutateAsync({ id })
      toast.success('确认成功')
      refetch()
    } catch (error) {
      toast.error('确认失败')
    }
  }

  const handleResolve = async (id: string) => {
    try {
      await resolveMutation.mutateAsync({
        id,
        data: { resolutionType: 'Fixed' as ResolutionType },
      })
      toast.success('解决成功')
      refetch()
    } catch (error) {
      toast.error('解决失败')
    }
  }

  const handleBatchAcknowledge = async () => {
    if (selectedIds.length === 0) return
    try {
      await batchAcknowledgeMutation.mutateAsync({ alarmIds: selectedIds })
      toast.success(`已确认 ${selectedIds.length} 条报警`)
      setSelectedIds([])
      refetch()
    } catch (error) {
      toast.error('批量确认失败')
    }
  }

  const handleBatchResolve = async () => {
    if (selectedIds.length === 0) return
    try {
      await batchResolveMutation.mutateAsync({
        alarmIds: selectedIds,
        resolutionType: 'Fixed' as ResolutionType,
      })
      toast.success(`已解决 ${selectedIds.length} 条报警`)
      setSelectedIds([])
      refetch()
    } catch (error) {
      toast.error('批量解决失败')
    }
  }

  const toggleSelection = (id: string) => {
    setSelectedIds(prev =>
      prev.includes(id) ? prev.filter(i => i !== id) : [...prev, id]
    )
  }

  const toggleSelectAll = () => {
    if (!data?.data) return
    if (selectedIds.length === data.data.length) {
      setSelectedIds([])
    } else {
      setSelectedIds(data.data.map(alarm => alarm.id))
    }
  }

  const getLevelColor = (level: string) => {
    switch (level.toLowerCase()) {
      case 'critical':
        return 'text-text-destructive bg-background-destructive-subtle'
      case 'error':
        return 'text-text-warning bg-background-warning-subtle'
      case 'warning':
        return 'text-text-warning bg-background-warning-subtle'
      case 'info':
        return 'text-text-accent bg-background-accent-subtle'
      default:
        return 'text-text-tertiary bg-components-panel-bg-alt'
    }
  }

  const getStatusColor = (status: string) => {
    switch (status.toLowerCase()) {
      case 'active':
        return 'text-text-destructive bg-background-destructive-subtle'
      case 'acknowledged':
        return 'text-text-warning bg-background-warning-subtle'
      case 'resolved':
        return 'text-text-success bg-background-success-subtle'
      default:
        return 'text-text-tertiary bg-components-panel-bg-alt'
    }
  }

  if (isLoading) {
    return (
      <div className="flex items-center justify-center h-64">
        <Loading />
      </div>
    )
  }

  return (
    <div className="space-y-4">
      {/* 工具栏 */}
      <div className="flex items-center justify-between">
        <div className="flex items-center gap-2">
          <Button
            variant="secondary"
            size="small"
            onClick={toggleSelectAll}
            disabled={!data?.data?.length}
          >
            {selectedIds.length === data?.data?.length ? '取消全选' : '全选'}
          </Button>
          {selectedIds.length > 0 && (
            <>
              <Button
                variant="secondary"
                size="small"
                onClick={handleBatchAcknowledge}
              >
                <RiCheckLine className="w-4 h-4 mr-1" />
                批量确认 ({selectedIds.length})
              </Button>
              <Button
                variant="secondary"
                size="small"
                onClick={handleBatchResolve}
              >
                <RiCloseLine className="w-4 h-4 mr-1" />
                批量解决 ({selectedIds.length})
              </Button>
            </>
          )}
        </div>
        <Button variant="secondary" size="small">
          <RiFilterLine className="w-4 h-4 mr-1" />
          筛选
        </Button>
      </div>

      {/* 报警列表 */}
      <div className="space-y-2">
        {data?.data?.map(alarm => (
          <div
            key={alarm.id}
            className="border border-divider-subtle rounded-lg p-4 bg-components-panel-bg hover:shadow-md transition-shadow"
          >
            <div className="flex items-start gap-4">
              <input
                type="checkbox"
                checked={selectedIds.includes(alarm.id)}
                onChange={() => toggleSelection(alarm.id)}
                className="mt-1"
              />
              <div className="flex-1 space-y-2">
                <div className="flex items-center justify-between">
                  <div className="flex items-center gap-2">
                    <RiAlarmWarningLine className="w-5 h-5 text-text-destructive" />
                    <span className="font-medium text-text-primary">{alarm.message}</span>
                    <span className={`px-2 py-1 rounded text-xs ${getLevelColor(alarm.alarmLevel)}`}>
                      {alarm.alarmLevel}
                    </span>
                    <span className={`px-2 py-1 rounded text-xs ${getStatusColor(alarm.status)}`}>
                      {alarm.status}
                    </span>
                  </div>
                  <div className="flex items-center gap-2">
                    {alarm.status === 'Active' && (
                      <>
                        <Button
                          variant="secondary"
                          size="small"
                          onClick={() => handleAcknowledge(alarm.id)}
                        >
                          确认
                        </Button>
                        <Button
                          variant="primary"
                          size="small"
                          onClick={() => handleResolve(alarm.id)}
                        >
                          解决
                        </Button>
                      </>
                    )}
                    {alarm.status === 'Acknowledged' && (
                      <Button
                        variant="primary"
                        size="small"
                        onClick={() => handleResolve(alarm.id)}
                      >
                        解决
                      </Button>
                    )}
                  </div>
                </div>
                <div className="text-sm text-text-secondary space-y-1">
                  <div>设备: {alarm.deviceName || alarm.deviceId}</div>
                  {alarm.propertyName && <div>属性: {alarm.propertyName}</div>}
                  {alarm.alarmValue && <div>当前值: {alarm.alarmValue}</div>}
                  {alarm.thresholdValue && <div>阈值: {alarm.thresholdValue}</div>}
                  <div>时间: {new Date(alarm.alarmTime).toLocaleString()}</div>
                </div>
              </div>
            </div>
          </div>
        ))}
      </div>

      {/* 分页 */}
      {data?.pagination && (
        <div className="flex items-center justify-between pt-4">
          <div className="text-sm text-text-tertiary">
            共 {data.pagination.totalCount} 条记录
          </div>
          <div className="flex items-center gap-2">
            <Button
              variant="secondary"
              size="small"
              disabled={filters.page === 1}
              onClick={() => setFilters(prev => ({ ...prev, page: (prev.page || 1) - 1 }))}
            >
              上一页
            </Button>
            <span className="text-sm text-text-secondary">
              {filters.page} / {data.pagination.totalPages}
            </span>
            <Button
              variant="secondary"
              size="small"
              disabled={filters.page === data.pagination.totalPages}
              onClick={() => setFilters(prev => ({ ...prev, page: (prev.page || 1) + 1 }))}
            >
              下一页
            </Button>
          </div>
        </div>
      )}
    </div>
  )
}

export default AlarmList
