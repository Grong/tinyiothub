export interface CommonResponse {
  result: 'success' | 'fail'
  message?: string
}

export interface PaginationResponse<T> {
  data: T[]
  total: number
  page: number
  page_size: number
  has_next: boolean
  has_prev: boolean
}

export interface User {
  id: string
  name: string
  email: string
  avatar?: string
  role: string
  status: 'active' | 'inactive'
  created_at: string
  updated_at: string
}

export interface Device {
  id: string
  name: string
  type: string
  status: 'online' | 'offline' | 'error'
  ip_address?: string
  port?: number
  description?: string
  tags: string[]
  created_at: string
  updated_at: string
  last_seen?: string
}

export interface DeviceProperty {
  id: string
  device_id: string
  name: string
  value: any
  data_type: 'string' | 'number' | 'boolean' | 'object'
  unit?: string
  timestamp: string
}

export interface DeviceAlarm {
  id: string
  device_id: string
  device_name: string
  level: 'info' | 'warning' | 'error' | 'critical'
  message: string
  status: 'active' | 'acknowledged' | 'resolved'
  created_at: string
  updated_at: string
}