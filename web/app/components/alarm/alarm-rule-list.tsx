'use client'

import { useState } from 'react'
import {
  RiAddLine,
  RiEditLine,
  RiDeleteBinLine,
  RiToggleLine,
} from '@remixicon/react'
import { useAlarmRules, useDeleteAlarmRule, useToggleAlarmRule } from '@/service/alarms'
import Button from '@/app/components/base/button'
import Loading from '@/app/components/base/loading'
import { useToast } from '@/hooks/use-toast'
import AlarmRuleForm from './alarm-rule-form'

interface AlarmRuleListProps {
  deviceId?: string
  onRuleCreated?: () => void
}

const AlarmRuleList: React.FC<AlarmRuleListProps> = ({ deviceId, onRuleCreated }) => {
  const { toast } = useToast()
  const [showForm, setShowForm] = useState(false)
  const [editingRule, setEditingRule] = useState<any>(null)

  const { data: rules, isLoading, refetch } = useAlarmRules(deviceId ? { deviceId } : undefined)
  const deleteMutation = useDeleteAlarmRule()
  const toggleMutation = useToggleAlarmRule()

  const handleDelete = async (id: string) => {
    if (!confirm('确定要删除这条规则吗？')) return
    
    try {
      await deleteMutation.mutateAsync(id)
      toast.success('删除成功')
      refetch()
    } catch (error) {
      toast.error('删除失败')
    }
  }

  const handleToggle = async (id: string, enabled: boolean) => {
    try {
      await toggleMutation.mutateAsync({ id, enabled: !enabled })
      toast.success(enabled ? '已禁用' : '已启用')
      refetch()
    } catch (error) {
      toast.error('操作失败')
    }
  }

  const handleEdit = (rule: any) => {
    setEditingRule(rule)
    setShowForm(true)
  }

  const handleFormClose = () => {
    setShowForm(false)
    setEditingRule(null)
    refetch()
    onRuleCreated?.()
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

  const getRuleTypeLabel = (type: string) => {
    const types: Record<string, string> = {
      Threshold: '阈值',
      Range: '范围',
      Change: '变化',
      Duration: '持续时间',
      Composite: '组合',
    }
    return types[type] || type
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
        <h3 className="text-lg font-medium text-text-primary">报警规则</h3>
        <Button
          variant="primary"
          size="small"
          onClick={() => setShowForm(true)}
        >
          <RiAddLine className="w-4 h-4 mr-1" />
          创建规则
        </Button>
      </div>

      {/* 规则列表 */}
      <div className="space-y-2">
        {rules?.map(rule => (
          <div
            key={rule.id}
            className={`border border-divider-subtle rounded-lg p-4 ${
              rule.isEnabled ? 'bg-components-panel-bg' : 'bg-components-panel-bg-alt'
            }`}
          >
            <div className="flex items-start justify-between">
              <div className="flex-1 space-y-2">
                <div className="flex items-center gap-2">
                  <span className="font-medium text-text-primary">{rule.name}</span>
                  <span className={`px-2 py-1 rounded text-xs ${getLevelColor(rule.alarmLevel)}`}>
                    {rule.alarmLevel}
                  </span>
                  <span className="px-2 py-1 rounded text-xs bg-components-panel-bg-alt text-text-tertiary">
                    {getRuleTypeLabel(rule.ruleType)}
                  </span>
                  {!rule.isEnabled && (
                    <span className="px-2 py-1 rounded text-xs bg-components-panel-bg-alt text-text-tertiary">
                      已禁用
                    </span>
                  )}
                </div>
                {rule.description && (
                  <div className="text-sm text-text-secondary">{rule.description}</div>
                )}
                <div className="text-xs text-text-tertiary">
                  创建时间: {new Date(rule.createdAt).toLocaleString()}
                </div>
              </div>
              <div className="flex items-center gap-2">
                <Button
                  variant="secondary"
                  size="small"
                  onClick={() => handleToggle(rule.id, rule.isEnabled)}
                >
                  <RiToggleLine className="w-4 h-4" />
                </Button>
                <Button
                  variant="secondary"
                  size="small"
                  onClick={() => handleEdit(rule)}
                >
                  <RiEditLine className="w-4 h-4" />
                </Button>
                <Button
                  variant="secondary"
                  size="small"
                  onClick={() => handleDelete(rule.id)}
                >
                  <RiDeleteBinLine className="w-4 h-4" />
                </Button>
              </div>
            </div>
          </div>
        ))}
      </div>

      {/* 空状态 */}
      {rules?.length === 0 && (
        <div className="text-center py-12 text-text-tertiary">
          <div className="text-lg mb-2">暂无报警规则</div>
          <div className="text-sm">点击"创建规则"按钮添加第一条规则</div>
        </div>
      )}

      {/* 规则表单弹窗 */}
      {showForm && (
        <AlarmRuleForm
          rule={editingRule}
          deviceId={deviceId}
          onClose={handleFormClose}
        />
      )}
    </div>
  )
}

export default AlarmRuleList
