/**
 * 统一的查询键管理
 * 遵循 TanStack Query 最佳实践
 */

// 查询键工厂函数
export const queryKeys = {
  // 认证相关
  auth: {
    all: ['auth'] as const,
    profile: () => [...queryKeys.auth.all, 'profile'] as const,
    session: () => [...queryKeys.auth.all, 'session'] as const,
  },

  // 用户管理
  users: {
    all: ['users'] as const,
    lists: () => [...queryKeys.users.all, 'list'] as const,
    list: (filters: Record<string, any>) => [...queryKeys.users.lists(), { filters }] as const,
    details: () => [...queryKeys.users.all, 'detail'] as const,
    detail: (id: string) => [...queryKeys.users.details(), id] as const,
    statistics: () => [...queryKeys.users.all, 'statistics'] as const,
  },

  // 设备管理
  devices: {
    all: ['devices'] as const,
    lists: () => [...queryKeys.devices.all, 'list'] as const,
    list: (filters: Record<string, any>) => [...queryKeys.devices.lists(), { filters }] as const,
    details: () => [...queryKeys.devices.all, 'detail'] as const,
    detail: (id: string) => [...queryKeys.devices.details(), id] as const,
    profile: (id: string) => [...queryKeys.devices.detail(id), 'profile'] as const,
    // 注意：以下查询键已被移除，因为功能已整合到 profile 中：
    // - properties: 属性数据现在通过 profile 获取
    // - propertyHistory: 历史数据功能暂时移除
    // - commands: 指令数据现在通过 profile 获取
    // - commandExecutions: 执行历史功能暂时移除
    // - events: 事件数据现在通过 profile 获取
    alarms: (deviceId?: string) => deviceId 
      ? [...queryKeys.devices.detail(deviceId), 'alarms'] 
      : [...queryKeys.devices.all, 'alarms'] as const,
    
    // 设备监控相关查询键
    status: (deviceId: string) => [...queryKeys.devices.detail(deviceId), 'status'] as const,
    metrics: (deviceId: string) => [...queryKeys.devices.detail(deviceId), 'metrics'] as const,
    performance: (deviceId: string) => [...queryKeys.devices.detail(deviceId), 'performance'] as const,
    performanceHistory: (deviceId: string, hours: number) => 
      [...queryKeys.devices.detail(deviceId), 'performance', 'history', hours] as const,
    performanceAlerts: (deviceId: string) => 
      [...queryKeys.devices.detail(deviceId), 'performance', 'alerts'] as const,
    traces: (deviceId: string, params?: any) => 
      [...queryKeys.devices.detail(deviceId), 'traces', params || {}] as const,
    traceStatistics: (deviceId: string, days?: number) => 
      [...queryKeys.devices.detail(deviceId), 'traces', 'statistics', days || 7] as const,
  },

  // 告警管理
  alarms: {
    all: ['alarms'] as const,
    lists: () => [...queryKeys.alarms.all, 'list'] as const,
    list: (filters: Record<string, any>) => [...queryKeys.alarms.lists(), { filters }] as const,
    details: () => [...queryKeys.alarms.all, 'detail'] as const,
    detail: (id: string) => [...queryKeys.alarms.details(), id] as const,
    rules: () => [...queryKeys.alarms.all, 'rules'] as const,
  },

  // 监控相关
  monitoring: {
    all: ['monitoring'] as const,
    health: () => [...queryKeys.monitoring.all, 'health'] as const,
    metrics: () => [...queryKeys.monitoring.all, 'metrics'] as const,
    logs: () => [...queryKeys.monitoring.all, 'logs'] as const,
  },

  // 系统配置
  system: {
    all: ['system'] as const,
    features: () => [...queryKeys.system.all, 'features'] as const,
    config: () => [...queryKeys.system.all, 'config'] as const,
    tasks: () => [...queryKeys.system.all, 'tasks'] as const,
  },

  // Dashboard 相关
  dashboard: {
    all: ['dashboard'] as const,
    stats: ['monitoring', 'dashboard', 'stats'] as const,
    deviceDistribution: ['monitoring', 'dashboard', 'device-distribution'] as const,
    trends: (period: string) => ['monitoring', 'dashboard', 'trends', period] as const,
    protocols: ['monitoring', 'dashboard', 'protocols'] as const,
    alarms: (limit: number) => ['monitoring', 'dashboard', 'alarms', limit] as const,
    metrics: ['monitoring', 'dashboard', 'metrics'] as const,
    quickDevices: (limit: number) => ['monitoring', 'dashboard', 'quick-devices', limit] as const,
  },

  // 设备模板管理
  templates: {
    all: ['templates'] as const,
    lists: () => [...queryKeys.templates.all, 'list'] as const,
    list: (filters: Record<string, any>) => [...queryKeys.templates.lists(), { filters }] as const,
    details: () => [...queryKeys.templates.all, 'detail'] as const,
    detail: (id: string) => [...queryKeys.templates.details(), id] as const,
    categories: () => [...queryKeys.templates.all, 'categories'] as const,
  },

  // 驱动管理
  drivers: {
    all: ['drivers'] as const,
    lists: () => [...queryKeys.drivers.all, 'list'] as const,
    list: (filters: Record<string, any>) => [...queryKeys.drivers.lists(), { filters }] as const,
    details: () => [...queryKeys.drivers.all, 'detail'] as const,
    detail: (name: string) => [...queryKeys.drivers.details(), name] as const,
    config: (name: string) => [...queryKeys.drivers.detail(name), 'config'] as const,
    support: (name: string) => [...queryKeys.drivers.detail(name), 'support'] as const,
    names: () => [...queryKeys.drivers.all, 'names'] as const,
  },

  // 事件管理
  events: {
    all: ['events'] as const,
    lists: () => [...queryKeys.events.all, 'list'] as const,
    list: (params: Record<string, any>) => [...queryKeys.events.lists(), params] as const,
    details: () => [...queryKeys.events.all, 'detail'] as const,
    detail: (id: string) => [...queryKeys.events.details(), id] as const,
    realTime: (filter: Record<string, any>) => [...queryKeys.events.all, 'real-time', filter] as const,
    overview: (params: Record<string, any>) => [...queryKeys.events.all, 'overview', params] as const,
    statusSummary: () => [...queryKeys.events.all, 'status-summary'] as const,
  },

  // 市场管理
  marketplace: {
    all: ['marketplace'] as const,
    templates: ['marketplace', 'templates'] as const,
    template: (id: string) => ['marketplace', 'templates', id] as const,
    drivers: ['marketplace', 'drivers'] as const,
    driver: (id: string) => ['marketplace', 'drivers', id] as const,
  },
} as const

// 类型推导辅助
export type QueryKey = typeof queryKeys[keyof typeof queryKeys]