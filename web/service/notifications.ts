/**
 * 通知渠道管理服务
 * 
 * 提供通知渠道的CRUD操作
 */

import { apiGet, apiPost, apiPut, apiDelete } from '@/lib/api-client'

// === 类型定义 ===

export interface NotificationChannel {
  id: string
  name: string
  channel_type: 'email' | 'webhook' | 'sms' | 'dingtalk' | 'wechat'
  config: NotificationChannelConfig
  enabled: boolean
  created_at: string
  updated_at: string
}

export interface NotificationChannelConfig {
  // Email配置
  smtp_host?: string
  smtp_port?: number
  smtp_username?: string
  smtp_password?: string
  from_email?: string
  from_name?: string
  // Webhook配置
  url?: string
  method?: 'GET' | 'POST' | 'PUT'
  headers?: Record<string, string>
  // 通用配置
  secret?: string
}

export interface NotificationRule {
  id: string
  name: string
  description?: string
  event_type?: string
  event_subtype?: string
  event_level?: number
  device_filter?: DeviceFilter
  notification_methods: string[]
  recipients: string[]
  enabled: boolean
  created_at: string
  updated_at: string
}

export interface DeviceFilter {
  device_ids?: string[]
  device_types?: string[]
  tags?: string[]
}

export interface CreateChannelRequest {
  name: string
  channel_type: NotificationChannel['channel_type']
  config: NotificationChannelConfig
  enabled?: boolean
}

export interface UpdateChannelRequest {
  name?: string
  config?: NotificationChannelConfig
  enabled?: boolean
}

export interface CreateRuleRequest {
  name: string
  description?: string
  event_type?: string
  event_subtype?: string
  event_level?: number
  device_filter?: DeviceFilter
  notification_methods: string[]
  recipients: string[]
  enabled?: boolean
}

export interface UpdateRuleRequest {
  name?: string
  description?: string
  event_type?: string
  event_subtype?: string
  event_level?: number
  device_filter?: DeviceFilter
  notification_methods?: string[]
  recipients?: string[]
  enabled?: boolean
}

// === API 函数 ===

/**
 * 获取通知渠道列表
 */
export async function getNotificationChannels(): Promise<NotificationChannel[]> {
  const response = await apiGet<NotificationChannel[]>('notification-channels', {})
  return response.result || []
}

/**
 * 获取单个通知渠道
 */
export async function getNotificationChannel(id: string): Promise<NotificationChannel | null> {
  const response = await apiGet<NotificationChannel>(`notification-channels/${id}`, {})
  return response.result
}

/**
 * 创建通知渠道
 */
export async function createNotificationChannel(
  data: CreateChannelRequest
): Promise<NotificationChannel> {
  const response = await apiPost<NotificationChannel>('notification-channels', data)
  return response.result
}

/**
 * 更新通知渠道
 */
export async function updateNotificationChannel(
  id: string,
  data: UpdateChannelRequest
): Promise<NotificationChannel> {
  const response = await apiPut<NotificationChannel>(`notification-channels/${id}`, data)
  return response.result
}

/**
 * 删除通知渠道
 */
export async function deleteNotificationChannel(id: string): Promise<void> {
  await apiDelete(`notification-channels/${id}`)
}

/**
 * 测试通知渠道
 */
export async function testNotificationChannel(
  id: string,
  testData: { title: string; content: string }
): Promise<{ success: boolean; message: string }> {
  const response = await apiPost<{ success: boolean; message: string }>(
    `notification-channels/${id}/test`,
    testData
  )
  return response.result
}

/**
 * 获取通知规则列表
 */
export async function getNotificationRules(): Promise<NotificationRule[]> {
  const response = await apiGet<NotificationRule[]>('notifications/rules', {})
  return response.result || []
}

/**
 * 创建通知规则
 */
export async function createNotificationRule(
  data: CreateRuleRequest
): Promise<NotificationRule> {
  const response = await apiPost<NotificationRule>('notifications/rules', data)
  return response.result
}

/**
 * 更新通知规则
 */
export async function updateNotificationRule(
  id: string,
  data: UpdateRuleRequest
): Promise<NotificationRule> {
  const response = await apiPut<NotificationRule>(`notifications/rules/${id}`, data)
  return response.result
}

/**
 * 删除通知规则
 */
export async function deleteNotificationRule(id: string): Promise<void> {
  await apiDelete(`notifications/rules/${id}`)
}

/**
 * 启用/禁用通知规则
 */
export async function toggleNotificationRule(
  id: string,
  enabled: boolean
): Promise<NotificationRule> {
  return updateNotificationRule(id, { enabled })
}
