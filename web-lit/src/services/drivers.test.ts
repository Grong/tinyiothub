import { describe, it, expect, vi, beforeEach } from 'vitest'
import type { Driver, DriverConfigOption } from './drivers'

// Mock the api-client module
vi.mock('../lib/api-client', () => ({
  apiGet: vi.fn(),
  apiPost: vi.fn(),
  apiPut: vi.fn(),
  apiDelete: vi.fn(),
}))

import { apiGet } from '../lib/api-client'
import { driverApi } from './drivers'

describe('driverApi', () => {
  beforeEach(() => {
    vi.clearAllMocks()
  })

  describe('getDrivers', () => {
    it('calls apiGet with correct endpoint', async () => {
      const mockDrivers: Driver[] = [
        { name: 'modbus-tcp', version: '1.0.0', isLoaded: true, category: 'industrial' },
        { name: 'mqtt', version: '2.1.0', isLoaded: true, category: 'iot' },
      ]
      const mockResponse = { code: 0, msg: '', result: mockDrivers }
      ;(apiGet as ReturnType<typeof vi.fn>).mockResolvedValue(mockResponse)

      const result = await driverApi.getDrivers()

      expect(apiGet).toHaveBeenCalledWith('drivers/dynamic/list')
      expect(result.result).toHaveLength(2)
      expect(result.result![0].name).toBe('modbus-tcp')
    })

    it('returns empty array when result is null', async () => {
      const mockResponse = { code: 0, msg: '', result: null }
      ;(apiGet as ReturnType<typeof vi.fn>).mockResolvedValue(mockResponse)

      const result = await driverApi.getDrivers()

      expect(result.result).toBeNull()
    })

    it('returns empty array when result is not an array', async () => {
      // This is the bug scenario: API returns an object instead of array
      const mockResponse = { code: 0, msg: '', result: { data: 'not an array' } }
      ;(apiGet as ReturnType<typeof vi.fn>).mockResolvedValue(mockResponse)

      const result = await driverApi.getDrivers()

      // The api client passes through whatever the API returns
      expect(result.result).toEqual({ data: 'not an array' })
    })
  })

  describe('getDriverConfig', () => {
    it('calls apiGet with correct endpoint and driver name', async () => {
      const mockConfig: DriverConfigOption[] = [
        {
          name: 'host',
          label: 'Host',
          type: 'string',
          required: true,
          defaultValue: 'localhost',
        },
        {
          name: 'port',
          label: 'Port',
          type: 'number',
          required: true,
          defaultValue: '502',
        },
        {
          name: 'debug',
          label: 'Debug Mode',
          type: 'boolean',
          required: false,
          defaultValue: 'false',
        },
      ]
      const mockResponse = { code: 0, msg: '', result: mockConfig }
      ;(apiGet as ReturnType<typeof vi.fn>).mockResolvedValue(mockResponse)

      const result = await driverApi.getDriverConfig('modbus-tcp')

      expect(apiGet).toHaveBeenCalledWith('drivers/modbus-tcp/config')
      expect(result.result).toHaveLength(3)
      expect(result.result![0].name).toBe('host')
      expect(result.result![0].required).toBe(true)
    })

    it('handles error response gracefully', async () => {
      const mockResponse = { code: -1, msg: 'Driver not found', result: null }
      ;(apiGet as ReturnType<typeof vi.fn>).mockResolvedValue(mockResponse)

      const result = await driverApi.getDriverConfig('unknown-driver')

      expect(result.result).toBeNull()
    })
  })
})
