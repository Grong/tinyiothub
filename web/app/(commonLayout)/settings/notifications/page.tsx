'use client'

import { useState, useEffect } from 'react'
import { useTranslation } from 'react-i18next'
import {
  NotificationChannel,
  NotificationRule,
  getNotificationChannels,
  createNotificationChannel,
  updateNotificationChannel,
  deleteNotificationChannel,
  testNotificationChannel,
  getNotificationRules,
  createNotificationRule,
  updateNotificationRule,
  deleteNotificationRule,
} from '@/service/notifications'
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import { toast } from 'sonner'

// 通知渠道类型
const CHANNEL_TYPES = [
  { value: 'email', label: '邮箱', icon: '📧' },
  { value: 'webhook', label: 'Webhook', icon: '🔗' },
  { value: 'dingtalk', label: '钉钉', icon: '💬' },
  { value: 'wechat', label: '企业微信', icon: '💼' },
] as const

export default function NotificationsPage() {
  const { t } = useTranslation('common')
  const queryClient = useQueryClient()
  const [activeTab, setActiveTab] = useState<'channels' | 'rules'>('channels')
  const [showAddChannel, setShowAddChannel] = useState(false)
  const [showAddRule, setShowAddRule] = useState(false)

  // 获取渠道列表
  const { data: channels = [], isLoading: channelsLoading } = useQuery({
    queryKey: ['notification-channels'],
    queryFn: getNotificationChannels,
  })

  // 获取规则列表
  const { data: rules = [], isLoading: rulesLoading } = useQuery({
    queryKey: ['notification-rules'],
    queryFn: getNotificationRules,
  })

  // 创建渠道
  const createChannelMutation = useMutation({
    mutationFn: createNotificationChannel,
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ['notification-channels'] })
      setShowAddChannel(false)
      toast.success('渠道创建成功')
    },
    onError: () => {
      toast.error('渠道创建失败')
    },
  })

  // 更新渠道
  const updateChannelMutation = useMutation({
    mutationFn: ({ id, data }: { id: string; data: any }) =>
      updateNotificationChannel(id, data),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ['notification-channels'] })
      toast.success('渠道更新成功')
    },
  })

  // 删除渠道
  const deleteChannelMutation = useMutation({
    mutationFn: deleteNotificationChannel,
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ['notification-channels'] })
      toast.success('渠道删除成功')
    },
  })

  // 测试渠道
  const testChannelMutation = useMutation({
    mutationFn: ({ id, data }: { id: string; data: any }) =>
      testNotificationChannel(id, data),
    onSuccess: () => {
      toast.success('测试消息发送成功')
    },
    onError: () => {
      toast.error('测试消息发送失败')
    },
  })

  return (
    <div className="space-y-6">
      <div className="flex items-center justify-between">
        <div>
          <h1 className="text-2xl font-semibold text-text-primary">
            {t('pages.settings.notifications.title', '通知配置')}
          </h1>
          <p className="mt-1 text-sm text-text-secondary">
            {t('pages.settings.notifications.subtitle', '配置告警通知渠道和规则')}
          </p>
        </div>
      </div>

      {/* Tabs */}
      <div className="border-b border-divider-subtle">
        <nav className="-mb-px flex space-x-8">
          <button
            onClick={() => setActiveTab('channels')}
            className={`py-4 px-1 border-b-2 font-medium text-sm ${
              activeTab === 'channels'
                ? 'border-primary-500 text-primary-600'
                : 'border-transparent text-text-secondary hover:text-text-primary'
            }`}
          >
            通知渠道
            {channels.length > 0 && (
              <span className="ml-2 px-2 py-0.5 text-xs rounded-full bg-primary-100 text-primary-600">
                {channels.length}
              </span>
            )}
          </button>
          <button
            onClick={() => setActiveTab('rules')}
            className={`py-4 px-1 border-b-2 font-medium text-sm ${
              activeTab === 'rules'
                ? 'border-primary-500 text-primary-600'
                : 'border-transparent text-text-secondary hover:text-text-primary'
            }`}
          >
            通知规则
            {rules.length > 0 && (
              <span className="ml-2 px-2 py-0.5 text-xs rounded-full bg-primary-100 text-primary-600">
                {rules.length}
              </span>
            )}
          </button>
        </nav>
      </div>

      {/* 渠道列表 */}
      {activeTab === 'channels' && (
        <div className="space-y-4">
          <div className="flex justify-end">
            <button
              onClick={() => setShowAddChannel(true)}
              className="px-4 py-2 bg-primary-600 text-white rounded-lg hover:bg-primary-700 transition-colors"
            >
              + 添加渠道
            </button>
          </div>

          {channelsLoading ? (
            <div className="text-center py-8 text-text-secondary">加载中...</div>
          ) : channels.length === 0 ? (
            <div className="text-center py-12 bg-components-panel-bg rounded-lg border border-divider-subtle">
              <div className="text-4xl mb-4">📭</div>
              <p className="text-text-secondary">暂无通知渠道</p>
              <button
                onClick={() => setShowAddChannel(true)}
                className="mt-4 px-4 py-2 text-primary-600 hover:text-primary-700"
              >
                添加第一个渠道
              </button>
            </div>
          ) : (
            <div className="grid grid-cols-1 md:grid-cols-2 lg:grid-cols-3 gap-4">
              {channels.map((channel) => (
                <ChannelCard
                  key={channel.id}
                  channel={channel}
                  onTest={(id) =>
                    testChannelMutation.mutate({
                      id,
                      data: { title: '测试', content: '这是一条测试消息' },
                    })
                  }
                  onToggle={(enabled) =>
                    updateChannelMutation.mutate({
                      id: channel.id,
                      data: { enabled },
                    })
                  }
                  onDelete={(id) => {
                    if (confirm('确定要删除这个渠道吗？')) {
                      deleteChannelMutation.mutate(id)
                    }
                  }}
                />
              ))}
            </div>
          )}
        </div>
      )}

      {/* 规则列表 */}
      {activeTab === 'rules' && (
        <div className="space-y-4">
          <div className="flex justify-end">
            <button
              onClick={() => setShowAddRule(true)}
              className="px-4 py-2 bg-primary-600 text-white rounded-lg hover:bg-primary-700 transition-colors"
            >
              + 添加规则
            </button>
          </div>

          {rulesLoading ? (
            <div className="text-center py-8 text-text-secondary">加载中...</div>
          ) : rules.length === 0 ? (
            <div className="text-center py-12 bg-components-panel-bg rounded-lg border border-divider-subtle">
              <div className="text-4xl mb-4">📋</div>
              <p className="text-text-secondary">暂无通知规则</p>
              <button
                onClick={() => setShowAddRule(true)}
                className="mt-4 px-4 py-2 text-primary-600 hover:text-primary-700"
              >
                添加第一个规则
              </button>
            </div>
          ) : (
            <div className="space-y-3">
              {rules.map((rule) => (
                <RuleCard
                  key={rule.id}
                  rule={rule}
                  onToggle={(enabled) =>
                    updateNotificationRule(rule.id, { enabled })
                  }
                  onDelete={(id) => {
                    if (confirm('确定要删除这个规则吗？')) {
                      deleteNotificationRule(id)
                    }
                  }}
                />
              ))}
            </div>
          )}
        </div>
      )}

      {/* 添加渠道弹窗 */}
      {showAddChannel && (
        <AddChannelModal
          onClose={() => setShowAddChannel(false)}
          onSubmit={(data) => createChannelMutation.mutate(data)}
          isLoading={createChannelMutation.isPending}
        />
      )}

      {/* 添加规则弹窗 */}
      {showAddRule && (
        <AddRuleModal
          channels={channels}
          onClose={() => setShowAddRule(false)}
          onSubmit={(data) => createNotificationRule(data)}
          isLoading={false}
        />
      )}
    </div>
  )
}

// 渠道卡片组件
function ChannelCard({
  channel,
  onTest,
  onToggle,
  onDelete,
}: {
  channel: NotificationChannel
  onTest: (id: string) => void
  onToggle: (enabled: boolean) => void
  onDelete: (id: string) => void
}) {
  const typeInfo = CHANNEL_TYPES.find((t) => t.value === channel.channel_type)

  return (
    <div className="bg-components-panel-bg rounded-lg border border-divider-subtle p-4">
      <div className="flex items-start justify-between">
        <div className="flex items-center space-x-3">
          <div className="text-2xl">{typeInfo?.icon || '📨'}</div>
          <div>
            <h3 className="font-medium text-text-primary">{channel.name}</h3>
            <p className="text-sm text-text-secondary">{typeInfo?.label}</p>
          </div>
        </div>
        <label className="relative inline-flex items-center cursor-pointer">
          <input
            type="checkbox"
            checked={channel.enabled}
            onChange={(e) => onToggle(e.target.checked)}
            className="sr-only peer"
          />
          <div className="w-9 h-5 bg-gray-200 peer-focus:outline-none rounded-full peer peer-checked:after:translate-x-full peer-checked:after:border-white after:content-[''] after:absolute after:top-[2px] after:left-[2px] after:bg-white after:border-gray-300 after:border after:rounded-full after:h-4 after:w-4 after:transition-all peer-checked:bg-primary-600"></div>
        </label>
      </div>

      <div className="mt-4 flex space-x-2">
        <button
          onClick={() => onTest(channel.id)}
          className="flex-1 px-3 py-1.5 text-sm text-text-secondary hover:text-text-primary border border-divider-subtle rounded hover:bg-fill-component-hover transition-colors"
        >
          测试
        </button>
        <button
          onClick={() => onDelete(channel.id)}
          className="px-3 py-1.5 text-sm text-red-600 hover:text-red-700 border border-divider-subtle rounded hover:bg-red-50 transition-colors"
        >
          删除
        </button>
      </div>
    </div>
  )
}

// 规则卡片组件
function RuleCard({
  rule,
  onToggle,
  onDelete,
}: {
  rule: NotificationRule
  onToggle: (enabled: boolean) => void
  onDelete: (id: string) => void
}) {
  return (
    <div className="bg-components-panel-bg rounded-lg border border-divider-subtle p-4 flex items-center justify-between">
      <div className="flex-1">
        <div className="flex items-center space-x-2">
          <h3 className="font-medium text-text-primary">{rule.name}</h3>
          {rule.event_type && (
            <span className="px-2 py-0.5 text-xs bg-primary-100 text-primary-600 rounded">
              {rule.event_type}
            </span>
          )}
        </div>
        <p className="text-sm text-text-secondary mt-1">
          通知方式: {rule.notification_methods.join(', ')} | 接收人: {rule.recipients.join(', ')}
        </p>
      </div>

      <div className="flex items-center space-x-2">
        <label className="relative inline-flex items-center cursor-pointer">
          <input
            type="checkbox"
            checked={rule.enabled}
            onChange={(e) => onToggle(e.target.checked)}
            className="sr-only peer"
          />
          <div className="w-9 h-5 bg-gray-200 peer-focus:outline-none rounded-full peer peer-checked:after:translate-x-full peer-checked:after:border-white after:content-[''] after:absolute after:top-[2px] after:left-[2px] after:bg-white after:border-gray-300 after:border after:rounded-full after:h-4 after:w-4 after:transition-all peer-checked:bg-primary-600"></div>
        </label>
        <button
          onClick={() => onDelete(rule.id)}
          className="px-3 py-1.5 text-sm text-red-600 hover:text-red-700"
        >
          删除
        </button>
      </div>
    </div>
  )
}

// 添加渠道弹窗
function AddChannelModal({
  onClose,
  onSubmit,
  isLoading,
}: {
  onClose: () => void
  onSubmit: (data: any) => void
  isLoading: boolean
}) {
  const [name, setName] = useState('')
  const [type, setType] = useState<'email' | 'webhook'>('email')
  const [config, setConfig] = useState<any>({})

  const handleSubmit = (e: React.FormEvent) => {
    e.preventDefault()
    onSubmit({ name, channel_type: type, config, enabled: true })
  }

  return (
    <div className="fixed inset-0 bg-black/50 flex items-center justify-center z-50">
      <div className="bg-components-panel-bg rounded-lg w-full max-w-md p-6">
        <h2 className="text-lg font-semibold text-text-primary mb-4">添加通知渠道</h2>

        <form onSubmit={handleSubmit} className="space-y-4">
          <div>
            <label className="block text-sm font-medium text-text-secondary mb-1">
              渠道名称
            </label>
            <input
              type="text"
              value={name}
              onChange={(e) => setName(e.target.value)}
              className="w-full px-3 py-2 bg-fill-component-hover border border-divider-subtle rounded-lg text-text-primary"
              required
            />
          </div>

          <div>
            <label className="block text-sm font-medium text-text-secondary mb-1">
              渠道类型
            </label>
            <select
              value={type}
              onChange={(e) => setType(e.target.value as any)}
              className="w-full px-3 py-2 bg-fill-component-hover border border-divider-subtle rounded-lg text-text-primary"
            >
              {CHANNEL_TYPES.map((t) => (
                <option key={t.value} value={t.value}>
                  {t.icon} {t.label}
                </option>
              ))}
            </select>
          </div>

          {type === 'email' && (
            <>
              <div>
                <label className="block text-sm font-medium text-text-secondary mb-1">
                  SMTP服务器
                </label>
                <input
                  type="text"
                  placeholder="smtp.example.com"
                  onChange={(e) => setConfig({ ...config, smtp_host: e.target.value })}
                  className="w-full px-3 py-2 bg-fill-component-hover border border-divider-subtle rounded-lg text-text-primary"
                />
              </div>
              <div className="grid grid-cols-2 gap-4">
                <div>
                  <label className="block text-sm font-medium text-text-secondary mb-1">
                    端口
                  </label>
                  <input
                    type="number"
                    placeholder="587"
                    onChange={(e) => setConfig({ ...config, smtp_port: parseInt(e.target.value) })}
                    className="w-full px-3 py-2 bg-fill-component-hover border border-divider-subtle rounded-lg text-text-primary"
                  />
                </div>
                <div>
                  <label className="block text-sm font-medium text-text-secondary mb-1">
                    用户名
                  </label>
                  <input
                    type="text"
                    onChange={(e) => setConfig({ ...config, smtp_username: e.target.value })}
                    className="w-full px-3 py-2 bg-fill-component-hover border border-divider-subtle rounded-lg text-text-primary"
                  />
                </div>
              </div>
            </>
          )}

          {type === 'webhook' && (
            <div>
              <label className="block text-sm font-medium text-text-secondary mb-1">
                Webhook URL
              </label>
              <input
                type="url"
                placeholder="https://example.com/webhook"
                onChange={(e) => setConfig({ ...config, url: e.target.value })}
                className="w-full px-3 py-2 bg-fill-component-hover border border-divider-subtle rounded-lg text-text-primary"
              />
            </div>
          )}

          <div className="flex justify-end space-x-3 pt-4">
            <button
              type="button"
              onClick={onClose}
              className="px-4 py-2 text-text-secondary hover:text-text-primary"
            >
              取消
            </button>
            <button
              type="submit"
              disabled={isLoading}
              className="px-4 py-2 bg-primary-600 text-white rounded-lg hover:bg-primary-700 disabled:opacity-50"
            >
              {isLoading ? '创建中...' : '创建'}
            </button>
          </div>
        </form>
      </div>
    </div>
  )
}

// 添加规则弹窗
function AddRuleModal({
  channels,
  onClose,
  onSubmit,
  isLoading,
}: {
  channels: NotificationChannel[]
  onClose: () => void
  onSubmit: (data: any) => void
  isLoading: boolean
}) {
  const [name, setName] = useState('')
  const [eventType, setEventType] = useState('')
  const [methods, setMethods] = useState<string[]>([])
  const [recipients, setRecipients] = useState('')

  const handleSubmit = (e: React.FormEvent) => {
    e.preventDefault()
    onSubmit({
      name,
      event_type: eventType,
      notification_methods: methods,
      recipients: recipients.split(',').map((r) => r.trim()),
      enabled: true,
    })
  }

  return (
    <div className="fixed inset-0 bg-black/50 flex items-center justify-center z-50">
      <div className="bg-components-panel-bg rounded-lg w-full max-w-md p-6">
        <h2 className="text-lg font-semibold text-text-primary mb-4">添加通知规则</h2>

        <form onSubmit={handleSubmit} className="space-y-4">
          <div>
            <label className="block text-sm font-medium text-text-secondary mb-1">
              规则名称
            </label>
            <input
              type="text"
              value={name}
              onChange={(e) => setName(e.target.value)}
              className="w-full px-3 py-2 bg-fill-component-hover border border-divider-subtle rounded-lg text-text-primary"
              required
            />
          </div>

          <div>
            <label className="block text-sm font-medium text-text-secondary mb-1">
              触发事件
            </label>
            <select
              value={eventType}
              onChange={(e) => setEventType(e.target.value)}
              className="w-full px-3 py-2 bg-fill-component-hover border border-divider-subtle rounded-lg text-text-primary"
            >
              <option value="">选择事件类型</option>
              <option value="device_offline">设备离线</option>
              <option value="device_error">设备故障</option>
              <option value="alarm_triggered">告警触发</option>
              <option value="threshold_exceeded">阈值超限</option>
            </select>
          </div>

          <div>
            <label className="block text-sm font-medium text-text-secondary mb-1">
              通知方式
            </label>
            <div className="flex space-x-4">
              {['email', 'webhook'].map((m) => (
                <label key={m} className="flex items-center">
                  <input
                    type="checkbox"
                    checked={methods.includes(m)}
                    onChange={(e) => {
                      if (e.target.checked) {
                        setMethods([...methods, m])
                      } else {
                        setMethods(methods.filter((x) => x !== m))
                      }
                    }}
                    className="mr-2"
                  />
                  {m === 'email' ? '📧 邮箱' : '🔗 Webhook'}
                </label>
              ))}
            </div>
          </div>

          <div>
            <label className="block text-sm font-medium text-text-secondary mb-1">
              接收人 (多个用逗号分隔)
            </label>
            <input
              type="text"
              value={recipients}
              onChange={(e) => setRecipients(e.target.value)}
              placeholder="user@example.com, another@example.com"
              className="w-full px-3 py-2 bg-fill-component-hover border border-divider-subtle rounded-lg text-text-primary"
              required
            />
          </div>

          <div className="flex justify-end space-x-3 pt-4">
            <button
              type="button"
              onClick={onClose}
              className="px-4 py-2 text-text-secondary hover:text-text-primary"
            >
              取消
            </button>
            <button
              type="submit"
              disabled={isLoading}
              className="px-4 py-2 bg-primary-600 text-white rounded-lg hover:bg-primary-700 disabled:opacity-50"
            >
              {isLoading ? '创建中...' : '创建'}
            </button>
          </div>
        </form>
      </div>
    </div>
  )
}
