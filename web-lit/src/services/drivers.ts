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

  getDriverConfig: (name: string) =>
    apiGet<DriverConfigOption[]>(`drivers/${name}/config`),
}
