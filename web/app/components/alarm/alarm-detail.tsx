'use client'

import { useState } from 'react'
import {
  RiAlarmWarningLine,
  RiCheckLine,
  RiCloseLine,
  RiInformationLine,
} from '@remixicon/react'
import { useAlarm, useAcknowledgeAlarm, useResolveAlarm } from '@/service/alarms'
import type { ResolutionType } from '@/types/alarm'
import Button from '@/app/components/base/button'
import Loading from '@/app/components/base/loading'
import { useToast } from '@/hooks/use-toast'

interface AlarmDetailProps {
  alarmId: string
  onClose?: () => void
}

const AlarmDetail: React.FC<AlarmDetailProps> = ({ alarmId, onClose }) => {
  const { toast } = useToast()
  const [note, setNote] = useState('')
  const [resolutionType, setResolutionType] = useState<ResolutionType>('Fixed')

  const { data: alarm, isLoading, refetch } = useAlarm(alarmId)
  const acknowledgeMutation = useAcknowledgeAlarm()
  const resolveMutation = useResolveAlarm()

  const handleAcknowledge = async () => {
    try {
      await acknowledgeMutation.mutateAsync({
        id: alarmId,
        data: note ? { note } : undefined,
      })
      toast.success('确认成功')
      setNote('')
      refetch()
    } catch (error) {
      toast.error('确认失败')
    }
  }

  const handleResolve = async () => {
    try {
      await resolveMutation.mutateAsync({
        id: alarmId,
        data: { resolutionType, note: note || undefined },
      })
      toast.success('解决成功')
      setNote('')
      refetch()
      onClose?.()
    } catch (error) {
      toast.error('解决失败')
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

  if (!alarm) {
    return (
      <div className="text-center py-12 text-text-tertiary">
        报警不存在
      </div>
    )
  }

  return (
    <div className="space-y-6">
      {/* 标题和状态 */}
      <div className="flex items-start justify-between">
        <div className="flex items-center gap-3">
          <RiAlarmWarningLine className="w-8 h-8 text-text-destructive" />
          <div>
            <h2 className="text-xl font-semibold text-text-primary">{alarm.message}</h2>
            <div className="flex items-center gap-2 mt-1">
              <span className={`px-2 py-1 rounded text-xs ${getLevelColor(alarm.alarmLevel)}`}>
                {alarm.alarmLevel}
              </span>
              <span className={`px-2 py-1 rounded text-xs ${getStatusColor(alarm.status)}`}>
                {alarm.status}
              </span>
            </div>
          </div>
        </div>
        {onClose && (
          <button
            onClick={onClose}
            className="text-text-tertiary hover:text-text-secondary"
          >
            <RiCloseLine className="w-6 h-6" />
          </button>
        )}
      </div>

      {/* 基本信息 */}
      <div className="border border-divider-subtle rounded-lg p-4 bg-components-panel-bg space-y-3">
        <h3 className="font-medium text-text-primary flex items-center gap-2">
          <RiInformationLine className="w-5 h-5" />
          基本信息
        </h3>
        <div className="grid grid-cols-2 gap-4 text-sm">
          <div>
            <div className="text-text-tertiary">设备</div>
            <div className="font-medium text-text-primary">{alarm.deviceName || alarm.deviceId}</div>
          </div>
          {alarm.propertyName && (
            <div>
              <div className="text-text-tertiary">属性</div>
              <div className="font-medium text-text-primary">{alarm.propertyName}</div>
            </div>
          )}
          {alarm.alarmValue && (
            <div>
              <div className="text-text-tertiary">当前值</div>
              <div className="font-medium text-text-primary">{alarm.alarmValue}</div>
            </div>
          )}
          {alarm.thresholdValue && (
            <div>
              <div className="text-text-tertiary">阈值</div>
              <div className="font-medium text-text-primary">{alarm.thresholdValue}</div>
            </div>
          )}
          <div>
            <div className="text-text-tertiary">报警时间</div>
            <div className="font-medium text-text-primary">{new Date(alarm.alarmTime).toLocaleString()}</div>
          </div>
          <div>
            <div className="text-text-tertiary">创建时间</div>
            <div className="font-medium text-text-primary">{new Date(alarm.createdAt).toLocaleString()}</div>
          </div>
        </div>
      </div>

      {/* 确认信息 */}
      {alarm.isAcknowledged && (
        <div className="border border-divider-subtle rounded-lg p-4 space-y-3 bg-background-warning-subtle">
          <h3 className="font-medium text-text-primary flex items-center gap-2">
            <RiCheckLine className="w-5 h-5" />
            确认信息
          </h3>
          <div className="grid grid-cols-2 gap-4 text-sm">
            <div>
              <div className="text-text-tertiary">确认人</div>
              <div className="font-medium text-text-primary">{alarm.acknowledgedBy}</div>
            </div>
            <div>
              <div className="text-text-tertiary">确认时间</div>
              <div className="font-medium text-text-primary">
                {alarm.acknowledgedAt && new Date(alarm.acknowledgedAt).toLocaleString()}
              </div>
            </div>
            {alarm.acknowledgedNote && (
              <div className="col-span-2">
                <div className="text-text-tertiary">备注</div>
                <div className="font-medium text-text-primary">{alarm.acknowledgedNote}</div>
              </div>
            )}
          </div>
        </div>
      )}

      {/* 解决信息 */}
      {alarm.isResolved && (
        <div className="border border-divider-subtle rounded-lg p-4 space-y-3 bg-background-success-subtle">
          <h3 className="font-medium text-text-primary flex items-center gap-2">
            <RiCloseLine className="w-5 h-5" />
            解决信息
          </h3>
          <div className="grid grid-cols-2 gap-4 text-sm">
            <div>
              <div className="text-text-tertiary">解决人</div>
              <div className="font-medium text-text-primary">{alarm.resolvedBy}</div>
            </div>
            <div>
              <div className="text-text-tertiary">解决时间</div>
              <div className="font-medium text-text-primary">
                {alarm.resolvedAt && new Date(alarm.resolvedAt).toLocaleString()}
              </div>
            </div>
            {alarm.resolvedNote && (
              <div className="col-span-2">
                <div className="text-text-tertiary">备注</div>
                <div className="font-medium text-text-primary">{alarm.resolvedNote}</div>
              </div>
            )}
          </div>
        </div>
      )}

      {/* 操作区域 */}
      {(alarm.status === 'Active' || alarm.status === 'Acknowledged') && (
        <div className="border border-divider-subtle rounded-lg p-4 bg-components-panel-bg space-y-4">
          <h3 className="font-medium text-text-primary">操作</h3>
          
          {/* 备注输入 */}
          <div>
            <label className="block text-sm font-medium text-text-secondary mb-1">备注</label>
            <textarea
              value={note}
              onChange={(e) => setNote(e.target.value)}
              placeholder="请输入备注信息（可选）"
              className="w-full px-3 py-2 border border-divider-subtle rounded-lg bg-components-input-bg text-text-primary"
              rows={3}
            />
          </div>

          {/* 解决方式选择 */}
          {alarm.status === 'Acknowledged' && (
            <div>
              <label className="block text-sm font-medium text-text-secondary mb-1">解决方式</label>
              <select
                value={resolutionType}
                onChange={(e) => setResolutionType(e.target.value as ResolutionType)}
                className="w-full px-3 py-2 border border-divider-subtle rounded-lg bg-components-input-bg text-text-primary"
              >
                <option value="Fixed">已修复</option>
                <option value="FalseAlarm">误报</option>
                <option value="Ignored">忽略</option>
              </select>
            </div>
          )}

          {/* 操作按钮 */}
          <div className="flex items-center gap-2">
            {alarm.status === 'Active' && (
              <>
                <Button
                  variant="secondary"
                  onClick={handleAcknowledge}
                  disabled={acknowledgeMutation.isPending}
                >
                  <RiCheckLine className="w-4 h-4 mr-1" />
                  确认报警
                </Button>
                <Button
                  variant="primary"
                  onClick={handleResolve}
                  disabled={resolveMutation.isPending}
                >
                  <RiCloseLine className="w-4 h-4 mr-1" />
                  直接解决
                </Button>
              </>
            )}
            {alarm.status === 'Acknowledged' && (
              <Button
                variant="primary"
                onClick={handleResolve}
                disabled={resolveMutation.isPending}
              >
                <RiCloseLine className="w-4 h-4 mr-1" />
                解决报警
              </Button>
            )}
          </div>
        </div>
      )}
    </div>
  )
}

export default AlarmDetail
