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
    it('calls apiGet with correct endpoint and flattens static + dynamic drivers', async () => {
      // API returns { staticDrivers, dynamic } structure
      const mockResponse = {
        code: 0,
        msg: '',
        result: {
          staticDrivers: [
            { name: 'modbus-tcp', version: '1.0.0', isLoaded: true, category: 'industrial' },
          ],
          dynamic: [
            { name: 'mqtt', version: '2.1.0', isLoaded: true, category: 'iot' },
          ],
        },
      }
      ;(apiGet as ReturnType<typeof vi.fn>).mockResolvedValue(mockResponse)

      const result = await driverApi.getDrivers()

      expect(apiGet).toHaveBeenCalledWith('drivers/dynamic/list')
      expect(result.result).toHaveLength(2)
      expect(result.result![0].name).toBe('modbus-tcp')
      expect(result.result![1].name).toBe('mqtt')
    })

    it('handles empty static and dynamic arrays', async () => {
      const mockResponse = {
        code: 0,
        msg: '',
        result: { staticDrivers: [], dynamic: [] },
      }
      ;(apiGet as ReturnType<typeof vi.fn>).mockResolvedValue(mockResponse)

      const result = await driverApi.getDrivers()

      expect(result.result).toEqual([])
    })

    it('returns null when result is null', async () => {
      const mockResponse = { code: 0, msg: '', result: null }
      ;(apiGet as ReturnType<typeof vi.fn>).mockResolvedValue(mockResponse)

      const result = await driverApi.getDrivers()

      expect(result.result).toBeNull()
    })
  })

  describe('getDriverConfig', () => {
    it('calls apiGet with correct endpoint and driver name', async () => {
      // API returns snake_case nested in config_options
      const mockResponse = {
        code: 0,
        msg: '',
        result: {
          driverName: 'modbus-tcp',
          configOptions: [
            {
              name: 'host',
              label: 'Host',
              optionType: 'string',
              defaultValue: 'localhost',
              required: true,
              description: null,
            },
            {
              name: 'port',
              label: 'Port',
              optionType: 'number',
              defaultValue: '502',
              required: true,
              description: null,
            },
            {
              name: 'debug',
              label: 'Debug Mode',
              optionType: 'boolean',
              defaultValue: 'false',
              required: false,
              description: null,
            },
          ],
          defaultConfig: { host: 'localhost', port: '502' },
        },
      }
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
