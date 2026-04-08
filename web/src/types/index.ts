/**
 * IoT 平台类型定义
 * Single source of truth — 前端统一使用 camelCase
 */

// ==================== User ====================
export interface User {
  id: string;
  /** 后端返回 display_name，转换为 displayName */
  displayName?: string;
  phone?: string;
  email?: string;
  avatar?: string;
  dateLastLogon?: string;
  isDisabled: boolean;
  parentId?: string;
}

export interface UserProfile extends User {
  role?: string;
  permissions?: string[];
  createdAt?: string;
  updatedAt?: string;
}

export interface LoginRequest {
  username: string;
  password: string;
}

export interface LoginResponse {
  accessToken: string;
  tokenType: string;
  expiresIn: number;
  userInfo: User;
  workspaceId?: string;
}

export interface CreateUserRequest {
  name: string;
  username: string;
  password: string;
  email?: string;
  phone?: string;
  role?: string;
  parentId?: string;
}

export interface UpdateUserRequest {
  name?: string;
  email?: string;
  phone?: string;
  isDisabled?: boolean;
}

export interface ChangePasswordRequest {
  oldPassword: string;
  newPassword: string;
}

// ==================== SMS Auth ====================
export interface SmsSendRequest {
  phone: string;
}

export interface SmsSendResponse {
  expiresIn: number;
  message: string;
}

export interface SmsLoginRequest {
  phone: string;
  code: string;
}

export interface SmsLoginResponse {
  accessToken: string;
  tokenType: string;
  expiresIn: number;
  userInfo: User;
  workspaceId?: string;
}

// ==================== WeChat Auth ====================
export interface WechatQrcodeResponse {
  qrcodeUrl: string;
  authorizeUrl: string;
  state: string;
}

export interface WechatLoginRequest {
  code: string;
}

export interface WechatLoginResponse {
  accessToken: string;
  tokenType: string;
  expiresIn: number;
  userInfo: User;
  isNewUser: boolean;
  workspaceId?: string;
}

// ==================== Device ====================
export interface Device {
  id: string;
  name: string;
  displayName?: string;
  deviceType?: string;
  address?: string;
  description?: string;
  position?: string;
  driverName?: string;
  deviceModel?: string;
  protocolType?: string;
  factoryName?: string;
  linkedData?: string;
  driverOptions?: string;
  state?: number;
  parentId?: string;
  productId?: string;
  organizationId?: string;
  createdAt?: string;
  updatedAt?: string;
  status?: 'online' | 'offline' | 'error' | 'maintenance';
  tags?: Tag[];
  properties?: DeviceProperty[];
  productName?: string;
}

export interface DeviceProperty {
  id: string;
  deviceId: string;
  name: string;
  displayName?: string;
  value: any;
  currentValue?: any;
  dataType: string;
  unit?: string;
  description?: string;
  updatedAt: string;
  lastUpdateTime?: string;
  alarmStatus?: number;
  isReadOnly?: boolean;
  minValue?: number;
  maxValue?: number;
}

export interface DeviceCommand {
  id: string;
  deviceId: string;
  name: string;
  description?: string;
  parameters: Record<string, any>;
  createdAt: string;
}

export interface DeviceAlarm {
  id: string;
  deviceId: string;
  deviceName: string;
  level: 'info' | 'warning' | 'error' | 'critical';
  message: string;
  status: 'active' | 'acknowledged' | 'resolved';
  createdAt: string;
  acknowledgedAt?: string;
  resolvedAt?: string;
}

export interface DeviceListParams {
  page?: number;
  pageSize?: number;
  name?: string;
  state?: string;
  deviceType?: string;
  driverName?: string;
  productId?: string;
  enabled?: boolean;
  tagIds?: string[];
  status?: string;
  protocolType?: string;
}

export interface CreateDeviceRequest {
  name: string;
  type?: string;
  ipAddress?: string;
  port?: number;
  description?: string;
  tags?: string[];
  manufacturer?: string;
  model?: string;
  protocol?: string;
}

export interface UpdateDeviceRequest extends Partial<CreateDeviceRequest> {
  id: string;
}

export interface DriverConfigOption {
  label: string;
  name: string;
  defaultValue: string;
  optionType: 'string' | 'number' | 'boolean' | 'select';
  required: boolean;
  description?: string;
}

export interface DriverConfigResponse {
  driverName: string;
  configOptions: DriverConfigOption[];
  defaultConfig: Record<string, string>;
}

export interface DeviceEvent {
  id: string;
  deviceId: string;
  eventType: 'alarm' | 'warning' | 'info' | 'error' | 'status_change' | 'command_executed';
  level: 'info' | 'warning' | 'error' | 'critical';
  title: string;
  message: string;
  data?: Record<string, any>;
  source?: string;
  createdAt: string;
  acknowledgedAt?: string;
  resolvedAt?: string;
  status: 'active' | 'acknowledged' | 'resolved';
}

export interface DeviceProfile {
  device: Device;
  isOnline: boolean;
  properties: DeviceProperty[];
  commands: DeviceCommand[];
  recentEvents?: DeviceEvent[];
  overview: {
    totalProperties: number;
    onlineProperties: number;
    offlineProperties: number;
    readonlyProperties: number;
    writableProperties: number;
    totalCommands: number;
    totalEvents: number;
    activeAlarms: number;
    lastUpdateTime?: string;
  };
  generatedAt: string;
}

// ==================== Alarm ====================
export type AlarmLevel = 'Info' | 'Warning' | 'Error' | 'Critical';
export type AlarmStatus = 'Active' | 'Acknowledged' | 'Resolved' | 'Suppressed';
export type ResolutionType = 'Fixed' | 'FalseAlarm' | 'Ignored' | 'AutoResolved';
export type RuleType = 'threshold' | 'range' | 'change' | 'duration' | 'composite';
export type ComparisonOperator = 'greater_than' | 'less_than' | 'greater_than_or_equal' | 'less_than_or_equal' | 'equal' | 'not_equal';
export type ChangeType = 'increase' | 'decrease' | 'any';
export type LogicalOperator = 'and' | 'or' | 'not';

export type AlarmCondition =
  | { type: 'threshold'; operator: ComparisonOperator; value: number }
  | { type: 'range'; min?: number; max?: number; inclusive: boolean }
  | { type: 'change'; changeType: ChangeType; threshold: number; timeWindow: number }
  | { type: 'duration'; condition: AlarmCondition; duration: number }
  | { type: 'composite'; operator: LogicalOperator; conditions: AlarmCondition[] };

export type NotificationChannelType = 'Email' | 'Sms' | 'Webhook' | 'Sse';

export interface NotificationConfig {
  enabled: boolean;
  channels: NotificationChannelType[];
  recipients: string[];
  suppressDuration?: number;
  repeatInterval?: number;
}

export interface Alarm {
  id: string;
  deviceId: string;
  deviceName?: string;
  propertyId?: string;
  propertyName?: string;
  ruleId?: string;
  ruleName?: string;
  alarmType: string;
  alarmLevel: string;
  message: string;
  alarmValue?: string;
  thresholdValue?: string;
  alarmTime: string;
  status: string;
  isAcknowledged: boolean;
  acknowledgedBy?: string;
  acknowledgedAt?: string;
  acknowledgedNote?: string;
  isResolved: boolean;
  resolvedBy?: string;
  resolvedAt?: string;
  resolvedNote?: string;
  createdAt: string;
}

export interface AlarmRule {
  id: string;
  name: string;
  description?: string;
  deviceId?: string;
  propertyId?: string;
  ruleType: string;
  condition: AlarmCondition;
  alarmLevel: string;
  isEnabled: boolean;
  notificationConfig: NotificationConfig;
  createdAt: string;
  updatedAt: string;
}

export interface AlarmStatistics {
  totalCount: number;
  activeCount: number;
  acknowledgedCount: number;
  resolvedCount: number;
}

export interface AlarmQueryParams {
  deviceIds?: string[];
  levels?: string[];
  statuses?: string[];
  startTime?: string;
  endTime?: string;
  page?: number;
  pageSize?: number;
}

export interface CreateAlarmRuleRequest {
  name: string;
  description?: string;
  deviceId?: string;
  propertyId?: string;
  ruleType: RuleType;
  condition: AlarmCondition;
  alarmLevel: AlarmLevel;
  notificationConfig: NotificationConfig;
}

export interface UpdateAlarmRuleRequest {
  name?: string;
  description?: string;
  condition?: AlarmCondition;
  alarmLevel?: AlarmLevel;
  notificationConfig?: NotificationConfig;
}

export interface AcknowledgeRequest {
  note?: string;
}

export interface ResolveRequest {
  resolutionType: ResolutionType;
  note?: string;
}

export interface BatchAcknowledgeRequest {
  alarmIds: string[];
}

export interface BatchResolveRequest {
  alarmIds: string[];
  resolutionType: ResolutionType;
}

export interface BatchOperationResult {
  successCount: number;
  totalCount: number;
}

// ==================== Dashboard ====================
export interface DashboardStats {
  totalDevices: number;
  onlineDevices: number;
  activeAlarms: number;
  systemStatus: 'healthy' | 'warning' | 'error';
  systemUptime: number;
  todayMessages: number;
  monthlyGrowth: {
    devices: number;
    messages: number;
  };
}

export interface DeviceStatusDistribution {
  online: number;
  offline: number;
  error: number;
  maintenance: number;
}

export interface DataTrend {
  timestamp: string;
  value: number;
  label?: string;
}

export interface ProtocolUsage {
  protocol: string;
  count: number;
  percentage: number;
}

export interface RecentAlarm {
  id: string;
  deviceId: string;
  deviceName: string;
  level: 'info' | 'warning' | 'error' | 'critical';
  message: string;
  createdAt: string;
  status: 'active' | 'acknowledged' | 'resolved';
}

export interface DashboardMetrics {
  cpu: number;
  memory: number;
  disk: number;
  network: {
    inbound: number;
    outbound: number;
  };
}

export interface QuickDevice {
  id: string;
  name: string;
  status: 'online' | 'offline' | 'error' | 'maintenance';
  lastSeen: string;
  type: string;
}

export interface DashboardData {
  stats: DashboardStats;
  deviceDistribution: DeviceStatusDistribution;
  dataTrends: DataTrend[];
  protocolUsage: ProtocolUsage[];
  recentAlarms: RecentAlarm[];
  systemMetrics: DashboardMetrics;
  quickDevices: QuickDevice[];
}

// ==================== Tag ====================
export interface Tag {
  id: string;
  name: string;
  type: string;
  description?: string;
  color?: string;
  bindingCount?: number;
  createdBy?: string;
  createdAt: string;
  updatedAt?: string;
}

export interface TagBinding {
  id: string;
  tagId: string;
  targetId: string;
  createdBy?: string;
  createdAt: string;
}

export interface CreateTagRequest {
  name: string;
  type: string;
  description?: string;
  color?: string;
}

export interface UpdateTagRequest {
  name?: string;
}

export interface CreateTagBindingRequest {
  tagId: string;
  targetId: string;
  targetType: 'device';
}

export interface BatchTagBindingRequest {
  tagIds: string[];
  targetId: string;
}

export interface TagStats {
  total: number;
  byType: Record<string, number>;
}

// ==================== Template ====================
export interface Template {
  id: string;
  name: string;
  displayName?: Record<string, string> | string;
  description?: Record<string, string> | string;
  category: string;
  version: string;
  author?: string;
  manufacturer?: string;
  deviceType?: string;
  protocolType?: string;
  driverName?: string;
  isBuiltin?: boolean;
  tags?: string[];
  configuration?: Record<string, any>;
  properties?: TemplateProperty[];
  commands?: TemplateCommand[];
  createdAt?: string;
  updatedAt?: string;
}

export interface TemplateProperty {
  id: string;
  name: string;
  displayName?: Record<string, string> | string;
  description?: Record<string, string> | string;
  dataType: string;
  unit?: string;
  defaultValue?: any;
  minValue?: number;
  maxValue?: number;
  isReadOnly?: boolean;
  isRequired?: boolean;
}

export interface TemplateCommand {
  id: string;
  name: string;
  displayName?: Record<string, string> | string;
  description?: Record<string, string> | string;
  parameters?: TemplateCommandParameter[];
  isRequired?: boolean;
}

export interface TemplateCommandParameter {
  name: string;
  displayName?: Record<string, string> | string;
  description?: Record<string, string> | string;
  dataType: string;
  defaultValue?: any;
  isRequired?: boolean;
}

export interface TemplateListParams {
  page?: number;
  pageSize?: number;
  keyword?: string;
  category?: string;
  manufacturer?: string;
  protocolType?: string;
  deviceType?: string;
}

export interface CreateTemplateRequest {
  name: string;
  displayName?: Record<string, string>;
  description?: Record<string, string>;
  category: string;
  version: string;
  author?: string;
  manufacturer?: string;
  deviceType?: string;
  protocolType?: string;
  driverName?: string;
  configuration?: Record<string, any>;
  properties?: TemplateProperty[];
  commands?: TemplateCommand[];
}

export interface UpdateTemplateRequest extends Partial<CreateTemplateRequest> {
  id: string;
}

// ==================== System ====================
export interface SystemConfig {
  id: string;
  key: string;
  value: string;
  description?: string;
  category?: string;
  updatedAt: string;
}

export interface SystemTask {
  id: string;
  name: string;
  type: string;
  status: 'pending' | 'running' | 'completed' | 'failed';
  progress?: number;
  result?: any;
  createdAt: string;
  updatedAt: string;
}

export interface SystemHealth {
  status: 'healthy' | 'degraded' | 'unhealthy';
  checks: {
    [key: string]: {
      status: 'healthy' | 'degraded' | 'unhealthy';
      message?: string;
      lastChecked: string;
    };
  };
  timestamp: string;
}

export interface SystemMetrics {
  timestamp?: string;
  cpu: number;
  memory: number;
  disk: number;
  network: {
    inbound: number;
    outbound: number;
  };
  activeConnections?: number;
  uptime?: number;
}

export interface SystemFeatures {
  version?: string;
  buildDate?: string;
  environment?: string;
  features?: {
    [key: string]: boolean;
  };
}

export interface ComponentHealth {
  name: string;
  status: 'healthy' | 'degraded' | 'unhealthy';
  message?: string;
  lastChecked?: string;
}

export interface HealthStatus {
  status: 'healthy' | 'degraded' | 'unhealthy';
  components: ComponentHealth[];
  timestamp: string;
}
