// 系统功能特性类型定义

export enum DatasetAttr {
  DATA_API_PREFIX = 'data-api-prefix',
  DATA_PUBLIC_API_PREFIX = 'data-public-api-prefix',
  DATA_PUBLIC_EDITION = 'data-public-edition',
  DATA_PUBLIC_SITE_ABOUT = 'data-public-site-about',
}

export interface SystemFeatures {
  // 系统版本信息
  version?: string
  edition?: string
  buildTime?: string
  
  // 功能开关
  enableDeviceManagement?: boolean
  enableAlarmSystem?: boolean
  enableMonitoring?: boolean
  enableUserManagement?: boolean
  enableSystemSettings?: boolean
  
  // API 配置
  apiPrefix?: string
  publicApiPrefix?: string
  
  // 系统限制
  maxDevices?: number
  maxUsers?: number
  maxAlarmRules?: number
  
  // 界面配置
  theme?: 'light' | 'dark' | 'system'
  language?: string
  timezone?: string
  
  // 高级功能
  enableAdvancedAnalytics?: boolean
  enableCustomDashboard?: boolean
  enableDataExport?: boolean
  enableApiAccess?: boolean
  
  // 安全配置
  enableTwoFactorAuth?: boolean
  sessionTimeout?: number
  passwordPolicy?: {
    minLength?: number
    requireUppercase?: boolean
    requireLowercase?: boolean
    requireNumbers?: boolean
    requireSpecialChars?: boolean
  }
  
  // 通知配置
  enableEmailNotifications?: boolean
  enableSmsNotifications?: boolean
  enableWebhookNotifications?: boolean
  
  // 系统状态
  systemStatus?: 'healthy' | 'degraded' | 'unhealthy'
  lastHealthCheck?: string
  
  // 许可证信息
  licenseType?: 'community' | 'professional' | 'enterprise'
  licenseExpiry?: string
  licensedFeatures?: string[]
}

// 默认系统功能配置
export const defaultSystemFeatures: SystemFeatures = {
  version: '1.0.0',
  edition: 'Community',
  
  // 基础功能默认开启
  enableDeviceManagement: true,
  enableAlarmSystem: true,
  enableMonitoring: true,
  enableUserManagement: true,
  enableSystemSettings: true,
  
  // API 配置
  apiPrefix: '/api/v1',
  publicApiPrefix: '/api/public',
  
  // 系统限制 (社区版)
  maxDevices: 100,
  maxUsers: 10,
  maxAlarmRules: 50,
  
  // 界面配置
  theme: 'system',
  language: 'zh-Hans',
  timezone: 'Asia/Shanghai',
  
  // 高级功能默认关闭 (需要专业版或企业版)
  enableAdvancedAnalytics: false,
  enableCustomDashboard: false,
  enableDataExport: false,
  enableApiAccess: false,
  
  // 安全配置
  enableTwoFactorAuth: false,
  sessionTimeout: 3600, // 1小时
  passwordPolicy: {
    minLength: 8,
    requireUppercase: false,
    requireLowercase: false,
    requireNumbers: false,
    requireSpecialChars: false,
  },
  
  // 通知配置
  enableEmailNotifications: false,
  enableSmsNotifications: false,
  enableWebhookNotifications: false,
  
  // 系统状态
  systemStatus: 'healthy',
  
  // 许可证信息
  licenseType: 'community',
  licensedFeatures: [
    'device-management',
    'alarm-system',
    'monitoring',
    'user-management',
    'system-settings',
  ],
}

// 功能检查工具函数
export const hasFeature = (features: SystemFeatures, feature: string): boolean => {
  return features.licensedFeatures?.includes(feature) ?? false
}

export const isFeatureEnabled = (features: SystemFeatures, feature: keyof SystemFeatures): boolean => {
  const value = features[feature]
  return typeof value === 'boolean' ? value : false
}

// 许可证类型检查
export const isProfessionalOrHigher = (features: SystemFeatures): boolean => {
  return features.licenseType === 'professional' || features.licenseType === 'enterprise'
}

export const isEnterprise = (features: SystemFeatures): boolean => {
  return features.licenseType === 'enterprise'
}

// 系统限制检查
export const canAddDevice = (features: SystemFeatures, currentDeviceCount: number): boolean => {
  return currentDeviceCount < (features.maxDevices ?? 0)
}

export const canAddUser = (features: SystemFeatures, currentUserCount: number): boolean => {
  return currentUserCount < (features.maxUsers ?? 0)
}

export const canAddAlarmRule = (features: SystemFeatures, currentRuleCount: number): boolean => {
  return currentRuleCount < (features.maxAlarmRules ?? 0)
}