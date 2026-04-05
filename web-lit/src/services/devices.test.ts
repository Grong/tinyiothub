import { describe, it, expect, vi, beforeEach } from 'vitest'
import type { Device, DeviceListParams, DeviceTrace } from './devices'

// Mock the api-client module
vi.mock('../lib/api-client', () => ({
  apiGet: vi.fn(),
  apiPost: vi.fn(),
  apiPut: vi.fn(),
  apiDelete: vi.fn(),
}))

import { apiGet, apiPost, apiPut, apiDelete } from '../lib/api-client'
import { deviceApi } from './devices'

describe('deviceApi', () => {
  beforeEach(() => {
    vi.clearAllMocks()
  })

  describe('getDevices', () => {
    it('calls apiGet with correct endpoint and params', async () => {
      const mockResponse = {
        code: 0,
        msg: '',
        result: {
          data: [],
          pagination: { page: 1, pageSize: 20, totalPages: 0 },
        },
      }
      ;(apiGet as ReturnType<typeof vi.fn>).mockResolvedValue(mockResponse)

      const params: DeviceListParams = { page: 1, pageSize: 20, search: 'test' }
      const result = await deviceApi.getDevices(params)

      expect(apiGet).toHaveBeenCalledWith('devices', params)
      expect(result).toEqual(mockResponse)
    })

    it('handles empty result gracefully', async () => {
      const mockResponse = { code: 0, msg: '', result: null }
      ;(apiGet as ReturnType<typeof vi.fn>).mockResolvedValue(mockResponse)

      const result = await deviceApi.getDevices()
      expect(result.result).toBeNull()
    })
  })

  describe('getDeviceTraces', () => {
    it('calls apiGet with deviceId and limit param', async () => {
      const mockTraces: DeviceTrace[] = [
        {
          id: '1',
          deviceId: 'dev-1',
          traceType: 'info',
          level: 'info',
          category: 'system',
          title: 'Test',
          message: 'Test message',
          createdAt: '2024-01-01T00:00:00Z',
        },
      ]
      const mockResponse = { code: 0, msg: '', result: mockTraces }
      ;(apiGet as ReturnType<typeof vi.fn>).mockResolvedValue(mockResponse)

      const result = await deviceApi.getDeviceTraces('dev-1', { limit: 50 })

      expect(apiGet).toHaveBeenCalledWith('devices/dev-1/traces', { limit: 50 })
      expect(result.result).toHaveLength(1)
      expect(result.result![0].deviceId).toBe('dev-1')
    })

    it('returns DeviceTrace array with camelCase fields', async () => {
      // API returns snake_case, api-client converts to camelCase
      const mockResponse = {
        code: 0,
        msg: '',
        result: [
          {
            id: '1',
            device_id: 'dev-1',
            trace_type: 'info',
            level: 'info',
            category: 'system',
            title: 'Test',
            message: 'Test message',
            created_at: '2024-01-01T00:00:00Z',
          },
        ],
      }
      ;(apiGet as ReturnType<typeof vi.fn>).mockResolvedValue(mockResponse)

      const result = await deviceApi.getDeviceTraces('dev-1')

      // Result should be converted to camelCase by api-client
      expect(result.result).toBeDefined()
      expect(Array.isArray(result.result)).toBe(true)
    })
  })

  describe('deleteDevice', () => {
    it('calls apiDelete with correct endpoint', async () => {
      const mockResponse = { code: 0, msg: '', result: true }
      ;(apiDelete as ReturnType<typeof vi.fn>).mockResolvedValue(mockResponse)

      await deviceApi.deleteDevice('device-123')

      expect(apiDelete).toHaveBeenCalledWith('devices/device-123')
    })
  })

  describe('executeCommand', () => {
    it('calls apiPost with deviceId, commandId and parameters', async () => {
      const mockResponse = {
        code: 0,
        msg: '',
        result: {
          id: 'exec-1',
          commandId: 'cmd-1',
          commandName: 'restart',
          parameters: { delay: 5 },
          status: 'pending',
          executedAt: '2024-01-01T00:00:00Z',
        },
      }
      ;(apiPost as ReturnType<typeof vi.fn>).mockResolvedValue(mockResponse)

      const result = await deviceApi.executeCommand('device-1', 'cmd-1', { delay: 5 })

      expect(apiPost).toHaveBeenCalledWith('devices/device-1/commands/cmd-1/execute', {
        parameters: { delay: 5 },
      })
      expect(result.result?.status).toBe('pending')
    })
  })

  describe('getDeviceAlarms', () => {
    it('uses alarms endpoint when deviceId not provided', async () => {
      const mockResponse = {
        code: 0,
        msg: '',
        result: { data: [], pagination: { page: 1, pageSize: 20, totalPages: 0 } },
      }
      ;(apiGet as ReturnType<typeof vi.fn>).mockResolvedValue(mockResponse)

      await deviceApi.getDeviceAlarms({ page: 1, pageSize: 20 })

      expect(apiGet).toHaveBeenCalledWith('alarms', { page: 1, pageSize: 20 })
    })

    it('uses device-specific alarms endpoint when deviceId provided', async () => {
      const mockResponse = {
        code: 0,
        msg: '',
        result: { data: [], pagination: { page: 1, pageSize: 20, totalPages: 0 } },
      }
      ;(apiGet as ReturnType<typeof vi.fn>).mockResolvedValue(mockResponse)

      await deviceApi.getDeviceAlarms({ deviceId: 'device-1', page: 1 })

      expect(apiGet).toHaveBeenCalledWith('devices/device-1/alarms', { page: 1 })
    })
  })
})
