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

export const driverApi = {
  getDrivers: () =>
    apiGet<Driver[]>('drivers/dynamic/list'),

  getDriverConfig: (name: string) =>
    apiGet<DriverConfigOption[]>(`drivers/${name}/config`),
}
