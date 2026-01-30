/**
 * 告警相关类型定义
 * 前端统一使用 camelCase 命名
 * 与后端 API 保持一致
 */

// 报警级别
export type AlarmLevel = 'Info' | 'Warning' | 'Error' | 'Critical'

// 报警状态
export type AlarmStatus = 'Active' | 'Acknowledged' | 'Resolved' | 'Suppressed'

// 报警类型
export type AlarmType = 'DeviceOffline' | 'DeviceError' | 'PropertyThreshold' | 'PropertyAnomaly' | 'CommandFailed' | string

// 解决方式
export type ResolutionType = 'Fixed' | 'FalseAlarm' | 'Ignored' | 'AutoResolved'

// 规则类型
export type RuleType = 'threshold' | 'range' | 'change' | 'duration' | 'composite'

// 比较运算符
export type ComparisonOperator = 'greater_than' | 'less_than' | 'greater_than_or_equal' | 'less_than_or_equal' | 'equal' | 'not_equal'

// 变化类型
export type ChangeType = 'increase' | 'decrease' | 'any'

// 逻辑运算符
export type LogicalOperator = 'and' | 'or' | 'not'

// 报警条件 - 使用内部标签格式
export type AlarmCondition = 
  | { type: 'threshold'; operator: ComparisonOperator; value: number }
  | { type: 'range'; min?: number; max?: number; inclusive: boolean }
  | { type: 'change'; changeType: ChangeType; threshold: number; timeWindow: number }
  | { type: 'duration'; condition: AlarmCondition; duration: number }
  | { type: 'composite'; operator: LogicalOperator; conditions: AlarmCondition[] }

// 通知渠道类型
export type NotificationChannelType = 'Email' | 'Sms' | 'Webhook' | 'Sse'

// 通知配置
export interface NotificationConfig {
  enabled: boolean
  channels: NotificationChannelType[]
  recipients: string[]
  suppressDuration?: number
  repeatInterval?: number
}

// 确认信息
export interface Acknowledgement {
  acknowledgedBy: string
  acknowledgedAt: string
  note?: string
}

// 解决信息
export interface Resolution {
  resolvedBy: string
  resolvedAt: string
  note?: string
  resolutionType: ResolutionType
}

// 报警实例
export interface Alarm {
  id: string
  deviceId: string
  deviceName?: string
  propertyId?: string
  propertyName?: string
  ruleId?: string
  ruleName?: string
  alarmType: string
  alarmLevel: string
  message: string
  alarmValue?: string
  thresholdValue?: string
  alarmTime: string
  status: string
  isAcknowledged: boolean
  acknowledgedBy?: string
  acknowledgedAt?: string
  acknowledgedNote?: string
  isResolved: boolean
  resolvedBy?: string
  resolvedAt?: string
  resolvedNote?: string
  createdAt: string
}

// 报警规则
export interface AlarmRule {
  id: string
  name: string
  description?: string
  deviceId?: string
  propertyId?: string
  ruleType: string
  condition: AlarmCondition
  alarmLevel: string
  isEnabled: boolean
  notificationConfig: NotificationConfig
  createdAt: string
  updatedAt: string
}

// 报警统计
export interface AlarmStatistics {
  totalCount: number
  activeCount: number
  acknowledgedCount: number
  resolvedCount: number
}

// 查询参数
export interface AlarmQueryParams {
  deviceIds?: string[]
  levels?: string[]
  statuses?: string[]
  startTime?: string
  endTime?: string
  page?: number
  pageSize?: number
}

export interface StatisticsQueryParams {
  startTime?: string
  endTime?: string
}

// 创建规则请求
export interface CreateAlarmRuleRequest {
  name: string
  description?: string
  deviceId?: string
  propertyId?: string
  ruleType: RuleType
  condition: AlarmCondition
  alarmLevel: AlarmLevel
  notificationConfig: NotificationConfig
}

// 更新规则请求
export interface UpdateAlarmRuleRequest {
  name?: string
  description?: string
  condition?: AlarmCondition
  alarmLevel?: AlarmLevel
  notificationConfig?: NotificationConfig
}

// 确认请求
export interface AcknowledgeRequest {
  note?: string
}

// 解决请求
export interface ResolveRequest {
  resolutionType: ResolutionType
  note?: string
}

// 批量确认请求
export interface BatchAcknowledgeRequest {
  alarmIds: string[]
}

// 批量解决请求
export interface BatchResolveRequest {
  alarmIds: string[]
  resolutionType: ResolutionType
}

// 批量操作结果
export interface BatchOperationResult {
  successCount: number
  totalCount: number
}