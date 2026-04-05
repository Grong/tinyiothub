import { describe, it, expect, vi, beforeEach } from 'vitest'
import type { Device, DeviceListParams, CreateDeviceRequest } from './devices'

// Mock the api-client module
vi.mock('../lib/api-client', () => ({
  apiGet: vi.fn(),
  apiPost: vi.fn(),
  apiPut: vi.fn(),
  apiDelete: vi.fn(),
}))

import { apiGet, apiPost, apiPut, apiDelete } from '../lib/api-client'
import { deviceApi } from './devices'

describe('deviceApi - additional coverage', () => {
  beforeEach(() => {
    vi.clearAllMocks()
  })

  describe('createDevice', () => {
    it('calls apiPost with device data', async () => {
      const createData: CreateDeviceRequest = {
        name: 'test-device',
        displayName: 'Test Device',
        protocol: 'modbus-tcp',
        address: '192.168.1.100:502',
        driverName: 'modbus-tcp',
        propertyValues: { temp: '25' },
        enabledCommands: ['restart'],
      }
      const mockResponse = {
        code: 0,
        msg: '',
        result: { id: 'new-1', name: 'test-device' } as Device,
      }
      ;(apiPost as ReturnType<typeof vi.fn>).mockResolvedValue(mockResponse)

      const result = await deviceApi.createDevice(createData)

      expect(apiPost).toHaveBeenCalledWith('devices', createData)
      expect(result.result?.id).toBe('new-1')
    })
  })

  describe('updateDevice', () => {
    it('calls apiPut with device id and partial data', async () => {
      const updateData: Partial<CreateDeviceRequest> = { name: 'updated-name' }
      const mockResponse = {
        code: 0,
        msg: '',
        result: { id: 'dev-1', name: 'updated-name' } as Device,
      }
      ;(apiPut as ReturnType<typeof vi.fn>).mockResolvedValue(mockResponse)

      const result = await deviceApi.updateDevice('dev-1', updateData)

      expect(apiPut).toHaveBeenCalledWith('devices/dev-1', updateData)
      expect(result.result?.name).toBe('updated-name')
    })
  })

  describe('getDevice', () => {
    it('calls apiGet with device id', async () => {
      const mockResponse = {
        code: 0,
        msg: '',
        result: { id: 'dev-1', name: 'Test Device', status: 'online' } as Device,
      }
      ;(apiGet as ReturnType<typeof vi.fn>).mockResolvedValue(mockResponse)

      const result = await deviceApi.getDevice('dev-1')

      expect(apiGet).toHaveBeenCalledWith('devices/dev-1')
      expect(result.result?.status).toBe('online')
    })
  })

  describe('getDeviceProfile', () => {
    it('calls apiGet with profile endpoint', async () => {
      const mockResponse = {
        code: 0,
        msg: '',
        result: {
          device: { id: 'dev-1', name: 'Test' },
          isOnline: true,
          properties: [],
          commands: [],
          recentEvents: [],
          overview: {
            totalProperties: 0, onlineProperties: 0, offlineProperties: 0,
            readonlyProperties: 0, writableProperties: 0, totalCommands: 0,
            recentEventCount: 0, criticalEventCount: 0, errorEventCount: 0,
          },
          generatedAt: '2024-01-01T00:00:00Z',
        },
      }
      ;(apiGet as ReturnType<typeof vi.fn>).mockResolvedValue(mockResponse)

      const result = await deviceApi.getDeviceProfile('dev-1')

      expect(apiGet).toHaveBeenCalledWith('devices/dev-1/profile')
      expect(result.result?.isOnline).toBe(true)
    })
  })

  describe('acknowledgeAlarm', () => {
    it('calls apiPost with alarm acknowledge endpoint', async () => {
      const mockResponse = { code: 0, msg: '', result: true }
      ;(apiPost as ReturnType<typeof vi.fn>).mockResolvedValue(mockResponse)

      const result = await deviceApi.acknowledgeAlarm('alarm-1')

      expect(apiPost).toHaveBeenCalledWith('alarms/alarm-1/acknowledge')
      expect(result.result).toBe(true)
    })
  })

  describe('resolveAlarm', () => {
    it('calls apiPost with alarm resolve endpoint', async () => {
      const mockResponse = { code: 0, msg: '', result: true }
      ;(apiPost as ReturnType<typeof vi.fn>).mockResolvedValue(mockResponse)

      const result = await deviceApi.resolveAlarm('alarm-1')

      expect(apiPost).toHaveBeenCalledWith('alarms/alarm-1/resolve')
      expect(result.result).toBe(true)
    })
  })

  describe('getDeviceStatus', () => {
    it('calls apiGet with status endpoint', async () => {
      const mockResponse = {
        code: 0,
        msg: '',
        result: {
          device_id: 'dev-1',
          is_online: true,
          connection_quality: 95,
          last_check: '2024-01-01T00:00:00Z',
        },
      }
      ;(apiGet as ReturnType<typeof vi.fn>).mockResolvedValue(mockResponse)

      const result = await deviceApi.getDeviceStatus('dev-1')

      expect(apiGet).toHaveBeenCalledWith('devices/dev-1/status')
      expect(result.result).toBeDefined()
    })
  })

  describe('getDeviceMetrics', () => {
    it('calls apiGet with metrics endpoint', async () => {
      const mockResponse = {
        code: 0,
        msg: '',
        result: {
          device_id: 'dev-1',
          cpu_usage: 45.5,
          memory_usage: 62.1,
          timestamp: '2024-01-01T00:00:00Z',
        },
      }
      ;(apiGet as ReturnType<typeof vi.fn>).mockResolvedValue(mockResponse)

      const result = await deviceApi.getDeviceMetrics('dev-1')

      expect(apiGet).toHaveBeenCalledWith('devices/dev-1/metrics')
      expect(result.result?.cpu_usage).toBe(45.5)
    })
  })

  describe('getDevicePerformance', () => {
    it('calls apiGet with performance endpoint and hours param', async () => {
      const mockResponse = {
        code: 0,
        msg: '',
        result: {
          metric: 'cpu_usage',
          data: [{ timestamp: 1704067200, value: 45.5 }],
        },
      }
      ;(apiGet as ReturnType<typeof vi.fn>).mockResolvedValue(mockResponse)

      const result = await deviceApi.getDevicePerformance('dev-1', 24)

      expect(apiGet).toHaveBeenCalledWith('devices/dev-1/performance', { hours: 24 })
      expect(result.result?.metric).toBe('cpu_usage')
    })

    it('calls without hours param when not provided', async () => {
      const mockResponse = { code: 0, msg: '', result: { metric: 'cpu_usage', data: [] } }
      ;(apiGet as ReturnType<typeof vi.fn>).mockResolvedValue(mockResponse)

      await deviceApi.getDevicePerformance('dev-1')

      expect(apiGet).toHaveBeenCalledWith('devices/dev-1/performance', undefined)
    })
  })

  describe('getDevicePerformanceAlerts', () => {
    it('calls apiGet with performance alerts endpoint', async () => {
      const mockResponse = {
        code: 0,
        msg: '',
        result: [
          {
            id: 'alert-1',
            device_id: 'dev-1',
            alert_type: 'high_cpu',
            level: 'warning',
            message: 'CPU usage exceeded 80%',
            triggered_at: '2024-01-01T00:00:00Z',
          },
        ],
      }
      ;(apiGet as ReturnType<typeof vi.fn>).mockResolvedValue(mockResponse)

      const result = await deviceApi.getDevicePerformanceAlerts('dev-1')

      expect(apiGet).toHaveBeenCalledWith('devices/dev-1/performance/alerts')
      expect(result.result).toHaveLength(1)
      expect(result.result![0].alert_type).toBe('high_cpu')
    })
  })

  describe('getDeviceTraceStatistics', () => {
    it('calls apiGet with statistics endpoint', async () => {
      const mockResponse = {
        code: 0,
        msg: '',
        result: {
          device_id: 'dev-1',
          total_traces: 150,
          by_level: { info: 100, warning: 40, error: 10 },
          by_type: { connection: 50, data: 80, system: 20 },
          recent_24h: 25,
          recent_7d: 150,
        },
      }
      ;(apiGet as ReturnType<typeof vi.fn>).mockResolvedValue(mockResponse)

      const result = await deviceApi.getDeviceTraceStatistics('dev-1', 7)

      expect(apiGet).toHaveBeenCalledWith('devices/dev-1/traces/statistics', { days: 7 })
      expect(result.result?.recent_7d).toBe(150)
    })
  })
})
