// web-lit/src/ui/controllers/devices.ts
import type { AppViewState } from '../app-view-state'
import type { Device, DeviceListParams, CreateDeviceRequest } from '../types'
import { apiGet, apiPost, apiPut, apiDelete } from '../api-client'

interface DeviceListResponse {
  items: Device[]
  total: number
  page: number
  pageSize: number
  totalPages: number
}

export async function loadDevices(host: AppViewState, params?: DeviceListParams): Promise<void> {
  host.devicesLoading = true
  try {
    const mergedParams = { ...host.devicesParams, ...params }
    host.devicesParams = mergedParams
    const res = await apiGet<DeviceListResponse>('devices', mergedParams as Record<string, unknown>)
    if (res.result) {
      host.devices = res.result.items
      host.devicesPage = res.result.page
      host.devicesTotalPages = res.result.totalPages
    }
  } finally {
    host.devicesLoading = false
  }
}

export async function loadDevice(host: AppViewState, id: string): Promise<void> {
  host.deviceDetailLoading = true
  try {
    const res = await apiGet<Device>(`devices/${id}`)
    if (res.result) {
      host.currentDevice = res.result
    }
  } finally {
    host.deviceDetailLoading = false
  }
}

export async function createDevice(host: AppViewState, data: CreateDeviceRequest): Promise<Device | null> {
  const res = await apiPost<Device>('devices', data)
  if (res.result) {
    host.devices = [res.result, ...host.devices]
    return res.result
  }
  return null
}

export async function updateDevice(host: AppViewState, id: string, data: Partial<CreateDeviceRequest>): Promise<void> {
  const res = await apiPut<Device>(`devices/${id}`, data)
  if (res.result) {
    host.devices = host.devices.map(d => d.id === id ? res.result! : d)
    if (host.currentDevice?.id === id) {
      host.currentDevice = res.result
    }
  }
}

export async function deleteDevice(host: AppViewState, id: string): Promise<void> {
  await apiDelete(`devices/${id}`)
  host.devices = host.devices.filter(d => d.id !== id)
  if (host.currentDevice?.id === id) {
    host.currentDevice = null
  }
}

export async function executeCommand(host: AppViewState, deviceId: string, commandId: string, parameters: Record<string, unknown>): Promise<void> {
  await apiPost(`devices/${deviceId}/commands/${commandId}/execute`, parameters)
}

export async function loadDrivers(host: AppViewState): Promise<void> {
  const res = await apiGet<Array<{ name: string; version?: string; description?: string; isLoaded: boolean; category?: string }>>('drivers/dynamic/list')
  if (res.result) {
    host.drivers = res.result
  }
}
