'use client'

import { useState, useEffect } from 'react'
import { RiCloseLine } from '@remixicon/react'
import { useCreateAlarmRule, useUpdateAlarmRule } from '@/service/alarms'
import type { CreateAlarmRuleRequest, AlarmRule, AlarmLevel, RuleType, ComparisonOperator, AlarmCondition } from '@/types/alarm'
import Button from '@/app/components/base/button'
import Input from '@/app/components/base/input'
import { useToast } from '@/hooks/use-toast'

interface AlarmRuleFormProps {
  rule?: AlarmRule | null
  deviceId?: string
  onClose: () => void
}

const AlarmRuleForm: React.FC<AlarmRuleFormProps> = ({ rule, deviceId, onClose }) => {
  const { toast } = useToast()
  const createMutation = useCreateAlarmRule()
  const updateMutation = useUpdateAlarmRule()

  const [formData, setFormData] = useState<Partial<CreateAlarmRuleRequest>>({
    name: '',
    description: '',
    deviceId: deviceId || '',
    propertyId: '',
    ruleType: 'threshold' as RuleType,
    alarmLevel: 'Warning' as AlarmLevel,
    condition: {
      type: 'threshold',
      operator: 'greater_than' as ComparisonOperator,
      value: 0,
    },
    notificationConfig: {
      enabled: true,
      channels: ['Sse'],
      recipients: [],
    },
  })

  const [thresholdValue, setThresholdValue] = useState('0')
  const [thresholdOperator, setThresholdOperator] = useState<ComparisonOperator>('greater_than')

  useEffect(() => {
    if (rule) {
      setFormData({
        name: rule.name,
        description: rule.description,
        deviceId: rule.deviceId,
        propertyId: rule.propertyId,
        ruleType: rule.ruleType as RuleType,
        alarmLevel: rule.alarmLevel as AlarmLevel,
        condition: rule.condition,
        notificationConfig: rule.notificationConfig,
      })
      
      // 解析阈值条件
      if (rule.condition.type === 'threshold') {
        setThresholdValue(rule.condition.value.toString())
        setThresholdOperator(rule.condition.operator)
      }
    }
  }, [rule])

  const handleSubmit = async (e: React.FormEvent) => {
    e.preventDefault()

    if (!formData.name) {
      toast.error('请输入规则名称')
      return
    }

    // 构建条件对象
    const condition: AlarmCondition = {
      type: 'threshold',
      operator: thresholdOperator,
      value: parseFloat(thresholdValue),
    }

    const data: CreateAlarmRuleRequest = {
      name: formData.name,
      description: formData.description,
      deviceId: formData.deviceId,
      propertyId: formData.propertyId,
      ruleType: formData.ruleType!,
      alarmLevel: formData.alarmLevel!,
      condition,
      notificationConfig: formData.notificationConfig!,
    }

    try {
      if (rule) {
        await updateMutation.mutateAsync({
          id: rule.id,
          data: {
            name: data.name,
            description: data.description,
            condition: data.condition,
            alarmLevel: data.alarmLevel,
            notificationConfig: data.notificationConfig,
          },
        })
        toast.success('更新成功')
      } else {
        await createMutation.mutateAsync(data)
        toast.success('创建成功')
      }
      onClose()
    } catch (error) {
      toast.error(rule ? '更新失败' : '创建失败')
    }
  }

  return (
    <div className="fixed inset-0 bg-components-modal-mask flex items-center justify-center z-50">
      <div className="bg-components-panel-bg rounded-lg p-6 w-full max-w-2xl max-h-[90vh] overflow-y-auto border border-divider-subtle">
        <div className="flex items-center justify-between mb-6">
          <h2 className="text-xl font-semibold text-text-primary">
            {rule ? '编辑规则' : '创建规则'}
          </h2>
          <button
            onClick={onClose}
            className="text-text-tertiary hover:text-text-secondary"
          >
            <RiCloseLine className="w-6 h-6" />
          </button>
        </div>

        <form onSubmit={handleSubmit} className="space-y-4">
          {/* 基本信息 */}
          <div>
            <label className="block text-sm font-medium text-text-secondary mb-1">
              规则名称 <span className="text-text-destructive">*</span>
            </label>
            <Input
              value={formData.name}
              onChange={(e) => setFormData({ ...formData, name: e.target.value })}
              placeholder="请输入规则名称"
            />
          </div>

          <div>
            <label className="block text-sm font-medium text-text-secondary mb-1">描述</label>
            <textarea
              value={formData.description}
              onChange={(e) => setFormData({ ...formData, description: e.target.value })}
              placeholder="请输入规则描述"
              className="w-full px-3 py-2 border border-divider-subtle rounded-lg bg-components-input-bg text-text-primary"
              rows={3}
            />
          </div>

          {/* 设备和属性 */}
          {!deviceId && (
            <div>
              <label className="block text-sm font-medium text-text-secondary mb-1">设备ID</label>
              <Input
                value={formData.deviceId}
                onChange={(e) => setFormData({ ...formData, deviceId: e.target.value })}
                placeholder="留空表示全局规则"
              />
            </div>
          )}

          <div>
            <label className="block text-sm font-medium text-text-secondary mb-1">属性ID</label>
            <Input
              value={formData.propertyId}
              onChange={(e) => setFormData({ ...formData, propertyId: e.target.value })}
              placeholder="请输入属性ID"
            />
          </div>

          {/* 报警级别 */}
          <div>
            <label className="block text-sm font-medium text-text-secondary mb-1">
              报警级别 <span className="text-text-destructive">*</span>
            </label>
            <select
              value={formData.alarmLevel}
              onChange={(e) => setFormData({ ...formData, alarmLevel: e.target.value as AlarmLevel })}
              className="w-full px-3 py-2 border border-divider-subtle rounded-lg bg-components-input-bg text-text-primary"
            >
              <option value="Info">信息</option>
              <option value="Warning">警告</option>
              <option value="Error">错误</option>
              <option value="Critical">严重</option>
            </select>
          </div>

          {/* 规则类型 */}
          <div>
            <label className="block text-sm font-medium text-text-secondary mb-1">
              规则类型 <span className="text-text-destructive">*</span>
            </label>
            <select
              value={formData.ruleType}
              onChange={(e) => setFormData({ ...formData, ruleType: e.target.value as RuleType })}
              className="w-full px-3 py-2 border border-divider-subtle rounded-lg bg-components-input-bg text-text-primary"
              disabled={!!rule}
            >
              <option value="threshold">阈值</option>
              <option value="range">范围</option>
              <option value="change">变化</option>
              <option value="duration">持续时间</option>
              <option value="composite">组合</option>
            </select>
          </div>

          {/* 阈值条件 */}
          {formData.ruleType === 'threshold' && (
            <div className="space-y-2">
              <label className="block text-sm font-medium text-text-secondary">阈值条件</label>
              <div className="flex gap-2">
                <select
                  value={thresholdOperator}
                  onChange={(e) => setThresholdOperator(e.target.value as ComparisonOperator)}
                  className="px-3 py-2 border border-divider-subtle rounded-lg bg-components-input-bg text-text-primary"
                >
                  <option value="greater_than">大于 (&gt;)</option>
                  <option value="greater_than_or_equal">大于等于 (&gt;=)</option>
                  <option value="less_than">小于 (&lt;)</option>
                  <option value="less_than_or_equal">小于等于 (&lt;=)</option>
                  <option value="equal">等于 (=)</option>
                  <option value="not_equal">不等于 (!=)</option>
                </select>
                <Input
                  type="number"
                  value={thresholdValue}
                  onChange={(e) => setThresholdValue(e.target.value)}
                  placeholder="阈值"
                  className="flex-1"
                />
              </div>
            </div>
          )}

          {/* 通知配置 */}
          <div>
            <label className="block text-sm font-medium text-text-secondary mb-2">通知渠道</label>
            <div className="space-y-2">
              <label className="flex items-center gap-2 text-text-secondary">
                <input
                  type="checkbox"
                  checked={formData.notificationConfig?.channels.includes('Email')}
                  onChange={(e) => {
                    const channels = e.target.checked
                      ? [...(formData.notificationConfig?.channels || []), 'Email' as const]
                      : formData.notificationConfig?.channels.filter(c => c !== 'Email') || []
                    setFormData({
                      ...formData,
                      notificationConfig: {
                        ...formData.notificationConfig!,
                        channels,
                      },
                    })
                  }}
                />
                <span>邮件</span>
              </label>
              <label className="flex items-center gap-2 text-text-secondary">
                <input
                  type="checkbox"
                  checked={formData.notificationConfig?.channels.includes('Sms')}
                  onChange={(e) => {
                    const channels = e.target.checked
                      ? [...(formData.notificationConfig?.channels || []), 'Sms' as const]
                      : formData.notificationConfig?.channels.filter(c => c !== 'Sms') || []
                    setFormData({
                      ...formData,
                      notificationConfig: {
                        ...formData.notificationConfig!,
                        channels,
                      },
                    })
                  }}
                />
                <span>短信</span>
              </label>
              <label className="flex items-center gap-2 text-text-secondary">
                <input
                  type="checkbox"
                  checked={formData.notificationConfig?.channels.includes('Sse')}
                  onChange={(e) => {
                    const channels = e.target.checked
                      ? [...(formData.notificationConfig?.channels || []), 'Sse' as const]
                      : formData.notificationConfig?.channels.filter(c => c !== 'Sse') || []
                    setFormData({
                      ...formData,
                      notificationConfig: {
                        ...formData.notificationConfig!,
                        channels,
                      },
                    })
                  }}
                />
                <span>实时推送</span>
              </label>
            </div>
          </div>

          {/* 按钮 */}
          <div className="flex items-center justify-end gap-2 pt-4">
            <Button variant="secondary" onClick={onClose}>
              取消
            </Button>
            <Button variant="primary" type="submit">
              {rule ? '更新' : '创建'}
            </Button>
          </div>
        </form>
      </div>
    </div>
  )
}

export default AlarmRuleForm
