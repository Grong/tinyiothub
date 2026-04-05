import { describe, it, expect, vi, beforeEach } from 'vitest'

// Mock the api-client module
vi.mock('../lib/api-client', () => ({
  apiGet: vi.fn(),
}))

import { apiGet } from '../lib/api-client'
import { tagApi } from './tags'

describe('tagApi', () => {
  beforeEach(() => {
    vi.clearAllMocks()
  })

  describe('getTags', () => {
    it('calls apiGet with device type by default', async () => {
      const mockResponse = {
        code: 0,
        msg: '',
        result: [
          { id: '1', name: 'temperature', color: '#ff0000' },
          { id: '2', name: 'humidity', color: '#00ff00' },
        ],
      }
      ;(apiGet as ReturnType<typeof vi.fn>).mockResolvedValue(mockResponse)

      const result = await tagApi.getTags()

      expect(apiGet).toHaveBeenCalledWith('tags?type=device')
      expect(result.result).toHaveLength(2)
      expect(result.result![0].name).toBe('temperature')
    })

    it('calls apiGet with alarm type when specified', async () => {
      const mockResponse = { code: 0, msg: '', result: [{ id: '3', name: 'critical', color: '#ff0000' }] }
      ;(apiGet as ReturnType<typeof vi.fn>).mockResolvedValue(mockResponse)

      await tagApi.getTags('alarm')

      expect(apiGet).toHaveBeenCalledWith('tags?type=alarm')
    })

    it('handles empty result', async () => {
      const mockResponse = { code: 0, msg: '', result: [] }
      ;(apiGet as ReturnType<typeof vi.fn>).mockResolvedValue(mockResponse)

      const result = await tagApi.getTags()

      expect(result.result).toEqual([])
    })

    it('handles null result', async () => {
      const mockResponse = { code: 0, msg: '', result: null }
      ;(apiGet as ReturnType<typeof vi.fn>).mockResolvedValue(mockResponse)

      const result = await tagApi.getTags()

      expect(result.result).toBeNull()
    })
  })
})
