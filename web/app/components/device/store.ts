import { create } from 'zustand'

export type DeviceDetail = {
  id: string
  name: string
  description?: string
  state?: number
  status?: string
  productName?: string
  lastSeen?: number
  createdAt: number
  updatedAt?: number
  properties?: Array<{
    name: string
    value: string
    unit?: string
  }>
  tags?: Array<{
    id: string
    name: string
  }>
}

type DeviceStore = {
  deviceDetail?: DeviceDetail
  setDeviceDetail: (deviceDetail?: DeviceDetail) => void
  deviceSidebarExpand: string
  setDeviceSidebarExpand: (deviceSidebarExpand: string) => void
}

export const useStore = create<DeviceStore>((set) => ({
  deviceDetail: undefined,
  setDeviceDetail: (deviceDetail) => {
    set({ deviceDetail })
  },
  deviceSidebarExpand: 'expand',
  setDeviceSidebarExpand: (deviceSidebarExpand) => {
    set({ deviceSidebarExpand })
    localStorage.setItem('device-detail-collapse-or-expand', deviceSidebarExpand)
  },
}))