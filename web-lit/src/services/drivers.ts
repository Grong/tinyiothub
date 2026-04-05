/**
 * 驱动管理服务 - Pure async API functions
 */

import { apiGet } from '../lib/api-client'

export interface Driver {
  name: string
  version?: string
  description?: string
  isLoaded: boolean
  category?: string
}

export interface DriverConfigOption {
  name: string
  label: string
  type: 'string' | 'number' | 'boolean' | 'select'
  defaultValue?: string
  required: boolean
  description?: string
  options?: string[]  // for select type
}

// API returns { staticDrivers: Driver[], dynamic: Driver[] }
interface AllDriversResponse {
  staticDrivers: Driver[]
  dynamic: Driver[]
}

export const driverApi = {
  getDrivers: async (): Promise<{ result: Driver[] | null }> => {
    const res = await apiGet<AllDriversResponse>('drivers/dynamic/list')
    if (res.result) {
      return {
        result: [
          ...(res.result.staticDrivers || []),
          ...(res.result.dynamic || []),
        ],
      }
    }
    return { result: null }
  },

  getDriverConfig: async (name: string): Promise<{ result: DriverConfigOption[] | null }> => {
    // API returns { driverName, configOptions[], defaultConfig } with snake_case option fields
    const res = await apiGet<{
      driverName: string
      configOptions: Array<{
        name: string
        label: string
        optionType?: string
        defaultValue?: string
        required: boolean
        description?: string | null
        options?: string[]
      }>
      defaultConfig: Record<string, string>
    }>(`drivers/${name}/config`)
    if (res.result) {
      return {
        result: res.result.configOptions.map(opt => ({
          name: opt.name,
          label: opt.label,
          type: (opt.optionType || 'string') as DriverConfigOption['type'],
          defaultValue: opt.defaultValue,
          required: opt.required,
          description: opt.description ?? undefined,
          options: opt.options,
        })),
      }
    }
    return { result: null }
  },
}
