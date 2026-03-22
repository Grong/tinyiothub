/**
 * 服务层统一导出
 * 
 * 这个文件提供了所有服务的统一入口，便于管理和使用
 * 所有服务都遵循以下规范：
 * 1. 使用 TanStack Query 进行数据管理
 * 2. 使用统一的 API 客户端 (@/lib/api-client)
 * 3. 遵循 ApiResponse<T> 响应格式
 * 4. 提供 React Query hooks 和原始 API 函数
 */

// === 核心服务 ===
export * from './auth'
export * from './devices'
export * from './templates'
export * from './tenant'

// === 监控和告警 ===
export * from './device-monitoring'
export * from './alarms'
export * from './dashboard'

// === 系统管理 ===
export * from './system'
export * from './users'
export * from './drivers'

// === 数据管理 ===
export * from './tag'

// === 服务类型定义 ===
export interface ServiceConfig {
  baseURL?: string
  timeout?: number
  retries?: number
}

// === 服务状态 ===
export interface ServiceHealth {
  status: 'healthy' | 'degraded' | 'unhealthy'
  services: {
    auth: boolean
    devices: boolean
    monitoring: boolean
    system: boolean
  }
  lastCheck: string
}

/**
 * 服务使用指南：
 * 
 * 1. 认证服务 (auth.ts)
 *    - useLogin() - 用户登录
 *    - useLogout() - 用户登出
 *    - useProfile() - 获取用户资料
 * 
 * 2. 设备服务 (devices.ts)
 *    - useDevices() - 获取设备列表
 *    - useDeviceProfile() - 获取设备完整信息
 *    - useCreateDevice() - 创建设备
 * 
 * 3. 模板服务 (templates.ts)
 *    - useTemplates() - 获取模板列表
 *    - useTemplate() - 获取模板详情
 *    - useCreateDeviceFromTemplate() - 基于模板创建设备
 * 
 * 4. 监控服务 (device-monitoring.ts)
 *    - useDeviceMetrics() - 获取设备指标
 *    - useDevicePerformance() - 获取性能数据
 *    - useDeviceTraces() - 获取追踪记录
 * 
 * 5. 告警服务 (alarms.ts)
 *    - useAlarms() - 获取告警列表
 *    - useAcknowledgeAlarm() - 确认告警
 *    - useResolveAlarm() - 解决告警
 * 
 * 6. 仪表板服务 (dashboard.ts)
 *    - useDashboardData() - 获取仪表板数据
 *    - useSystemMetrics() - 获取系统指标
 * 
 * 7. 系统服务 (system.ts)
 *    - useSystemHealth() - 获取系统健康状态
 *    - useSystemConfig() - 获取系统配置
 * 
 * 8. 用户服务 (users.ts)
 *    - useUsers() - 获取用户列表
 *    - useCreateUser() - 创建用户
 * 
 * 9. 驱动服务 (drivers.ts)
 *    - useDrivers() - 获取驱动列表
 *    - useDriverConfig() - 获取驱动配置
 * 
 * 10. 标签服务 (tag.ts)
 *     - 提供标签管理的原始API函数
 *     - 建议后续迁移到 React Query hooks
 */