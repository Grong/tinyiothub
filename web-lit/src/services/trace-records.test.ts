import { describe, it, expect, vi, beforeEach } from 'vitest'
import type { DeviceTrace } from './devices'

// Mock the api-client module
vi.mock('../lib/api-client', () => ({
  apiGet: vi.fn(),
}))

import { apiGet } from '../lib/api-client'
import { deviceApi } from './devices'

describe('deviceApi - getDeviceTraces', () => {
  beforeEach(() => {
    vi.clearAllMocks()
  })

  it('returns traces with camelCase fields after conversion', async () => {
    // Mock returns snake_case (as the real API does)
    // api-client's apiGet applies keysToCamelCase to response.result
    // So we simulate the final post-conversion shape
    const mockResponse = {
      code: 0,
      msg: '',
      result: [
        {
          id: 'trace-1',
          deviceId: 'dev-1',
          traceType: 'system',
          level: 'info',
          category: 'connection',
          title: 'Device connected',
          message: 'Device successfully connected to gateway',
          details: { ip: '192.168.1.100' },
          source: 'gateway',
          userId: 'user-1',
          sessionId: 'sess-1',
          createdAt: '2024-01-15T10:30:00Z',
        },
        {
          id: 'trace-2',
          deviceId: 'dev-1',
          traceType: 'error',
          level: 'error',
          category: 'data',
          title: 'Read timeout',
          message: 'Modbus read timeout after 5000ms',
          details: { address: '192.168.1.100', register: 100 },
          source: 'driver',
          createdAt: '2024-01-15T10:31:00Z',
        },
      ],
    }
    ;(apiGet as ReturnType<typeof vi.fn>).mockResolvedValue(mockResponse)

    const result = await deviceApi.getDeviceTraces('dev-1', { limit: 50 })

    expect(result.result).toBeDefined()
    expect(Array.isArray(result.result)).toBe(true)
    expect(result.result).toHaveLength(2)

    // Verify camelCase properties are accessible
    const trace1 = result.result![0]
    expect(trace1.id).toBe('trace-1')
    expect(trace1.deviceId).toBe('dev-1')
    expect(trace1.traceType).toBe('system')
    expect(trace1.createdAt).toBe('2024-01-15T10:30:00Z')

    const trace2 = result.result![1]
    expect(trace2.level).toBe('error')
    expect(trace2.createdAt).toBe('2024-01-15T10:31:00Z')
  })

  it('handles null result gracefully', async () => {
    const mockResponse = { code: 0, msg: '', result: null }
    ;(apiGet as ReturnType<typeof vi.fn>).mockResolvedValue(mockResponse)

    const result = await deviceApi.getDeviceTraces('dev-1')

    expect(result.result).toBeNull()
  })

  it('passes limit and offset params to API', async () => {
    const mockResponse = { code: 0, msg: '', result: [] }
    ;(apiGet as ReturnType<typeof vi.fn>).mockResolvedValue(mockResponse)

    await deviceApi.getDeviceTraces('dev-1', { limit: 100, offset: 20 })

    expect(apiGet).toHaveBeenCalledWith('devices/dev-1/traces', { limit: 100, offset: 20 })
  })

  it('passes trace_types filter to API', async () => {
    const mockResponse = { code: 0, msg: '', result: [] }
    ;(apiGet as ReturnType<typeof vi.fn>).mockResolvedValue(mockResponse)

    await deviceApi.getDeviceTraces('dev-1', { trace_types: ['error', 'warning'] })

    expect(apiGet).toHaveBeenCalledWith('devices/dev-1/traces', { trace_types: ['error', 'warning'] })
  })

  it('handles traces with null details', async () => {
    const mockResponse = {
      code: 0,
      msg: '',
      result: [
        {
          id: 'trace-3',
          deviceId: 'dev-1',
          traceType: 'info',
          level: 'info',
          category: 'system',
          title: 'Event',
          message: 'No details provided',
          details: null,
          createdAt: '2024-01-15T10:32:00Z',
        },
      ],
    }
    ;(apiGet as ReturnType<typeof vi.fn>).mockResolvedValue(mockResponse)

    const result = await deviceApi.getDeviceTraces('dev-1')

    expect(result.result![0].details).toBeNull()
    expect(result.result![0].createdAt).toBe('2024-01-15T10:32:00Z')
  })
})
