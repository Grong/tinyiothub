import { apiGet, apiPost, apiDelete } from '@/lib/api-client'
import { useQuery, useMutation, useQueryClient } from '@tanstack/react-query'
import { queryKeys } from '@/lib/query-keys'

// ==================== 类型定义 ====================

// 驱动基本信息
export interface Driver {
  name: string
  version?: string
  description?: string
  isLoaded: boolean
  path?: string
  category?: string
  tags?: string[]
}

// 驱动列表响应（结构化格式）
export interface AllDriversResponse {
  staticDrivers: Driver[]
  dynamic: Driver[]
}

// 驱动配置选项
export interface DriverConfigOption {
  name: string
  label: string
  type: string
  defaultValue: string
  required: boolean
  description?: string
}

// 驱动配置响应
export interface DriverConfigResponse {
  driverName: string
  configOptions: DriverConfigOption[]
  defaultConfig: Record<string, string>
}

// 加载驱动请求
export interface LoadDriverRequest {
  path: string
}

// ==================== API 调用函数 ====================

export const driverApi = {
  // 获取所有驱动（结构化格式：包括静态和动态）
  getAllDrivers: () => 
    apiGet<AllDriversResponse>('drivers/dynamic/list'),
  
  // 获取驱动配置
  getDriverConfig: (name: string) => 
    apiGet<DriverConfigResponse>(`drivers/${name}/config`),
  
  // 加载动态驱动
  loadDriver: (data: LoadDriverRequest) => 
    apiPost<string>('drivers/dynamic/load', data),
  
  // 卸载动态驱动
  unloadDriver: (name: string) => 
    apiDelete<boolean>(`drivers/dynamic/${name}/unload`),
  
  // 重新加载驱动目录
  reloadDriversDir: () => 
    apiPost<string[]>('drivers/dynamic/reload', {}),
}

// ==================== React Query Hooks ====================

/**
 * 获取所有驱动（结构化格式）
 * 用于：驱动管理页面、市场页面
 * 返回：{ staticDrivers: Driver[], dynamic: Driver[] }
 */
export const useAllDrivers = () => {
  return useQuery({
    queryKey: queryKeys.drivers.all,
    queryFn: async () => {
      const response = await driverApi.getAllDrivers()
      return response.result || { staticDrivers: [], dynamic: [] }
    },
  })
}

/**
 * 获取驱动列表（扁平格式）
 * 用于：设备创建页面的驱动下拉选择
 * 返回：Driver[]（合并了静态和动态驱动）
 */
export const useDriversList = () => {
  return useQuery({
    queryKey: queryKeys.drivers.lists(),
    queryFn: async () => {
      const response = await driverApi.getAllDrivers()
      const data = response.result
      if (!data) return []
      // 合并静态驱动和动态驱动，返回扁平列表
      return [...(data.staticDrivers || []), ...(data.dynamic || [])]
    },
  })
}

/**
 * 获取驱动配置参数
 * 用于：设备创建时显示驱动配置表单
 */
export const useDriverConfig = (driverName: string, enabled = true) => {
  return useQuery({
    queryKey: queryKeys.drivers.config(driverName),
    queryFn: async () => {
      const response = await driverApi.getDriverConfig(driverName)
      return response.result
    },
    enabled: enabled && !!driverName,
  })
}

/**
 * 加载驱动
 * 用于：驱动管理页面
 */
export const useLoadDriver = () => {
  const queryClient = useQueryClient()
  
  return useMutation({
    mutationFn: driverApi.loadDriver,
    onSuccess: () => {
      // 刷新所有驱动相关的查询
      queryClient.invalidateQueries({ queryKey: queryKeys.drivers.all })
      queryClient.invalidateQueries({ queryKey: queryKeys.drivers.lists() })
    },
  })
}

/**
 * 卸载驱动
 * 用于：驱动管理页面
 */
export const useUnloadDriver = () => {
  const queryClient = useQueryClient()
  
  return useMutation({
    mutationFn: driverApi.unloadDriver,
    onSuccess: () => {
      // 刷新所有驱动相关的查询
      queryClient.invalidateQueries({ queryKey: queryKeys.drivers.all })
      queryClient.invalidateQueries({ queryKey: queryKeys.drivers.lists() })
    },
  })
}

/**
 * 重新加载驱动目录
 * 用于：驱动管理页面
 */
export const useReloadDriversDir = () => {
  const queryClient = useQueryClient()
  
  return useMutation({
    mutationFn: driverApi.reloadDriversDir,
    onSuccess: () => {
      // 刷新所有驱动相关的查询
      queryClient.invalidateQueries({ queryKey: queryKeys.drivers.all })
      queryClient.invalidateQueries({ queryKey: queryKeys.drivers.lists() })
    },
  })
}

// ==================== 导出说明 ====================
// 
// 使用指南：
// 
// 1. 设备创建页面（需要扁平列表）：
//    import { useDriversList } from '@/service/drivers'
//    const { data: drivers } = useDriversList()
//    // drivers 是 Driver[]
// 
// 2. 市场页面（需要结构化数据）：
//    import { useAllDrivers } from '@/service/drivers'
//    const { data: driversData } = useAllDrivers()
//    // driversData 是 { staticDrivers: Driver[], dynamic: Driver[] }
// 
// 3. 驱动管理页面（需要结构化数据）：
//    import { useAllDrivers } from '@/service/drivers'
//    const { data: driversData } = useAllDrivers()
//    // driversData 是 { staticDrivers: Driver[], dynamic: Driver[] }
//
