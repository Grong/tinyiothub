import { describe, it, expect, vi, beforeEach } from 'vitest'

// Mock the api-client module
vi.mock('../lib/api-client', () => ({
  apiGet: vi.fn(),
  apiPost: vi.fn(),
  apiDelete: vi.fn(),
}))

import { apiGet, apiPost, apiDelete } from '../lib/api-client'
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

  describe('getResourceTags', () => {
    it('calls apiGet with correct endpoint', async () => {
      const mockResponse = {
        code: 0,
        msg: '',
        result: [
          { id: '1', name: 'temperature', color: '#ff0000', type: 'device', createdAt: '2024-01-01' },
        ],
      }
      ;(apiGet as ReturnType<typeof vi.fn>).mockResolvedValue(mockResponse)

      const result = await tagApi.getResourceTags('device-123')

      expect(apiGet).toHaveBeenCalledWith('tags/bindings/target/device-123')
      expect(result.result).toHaveLength(1)
      expect(result.result![0].name).toBe('temperature')
    })

    it('handles empty bindings', async () => {
      const mockResponse = { code: 0, msg: '', result: [] }
      ;(apiGet as ReturnType<typeof vi.fn>).mockResolvedValue(mockResponse)

      const result = await tagApi.getResourceTags('device-456')

      expect(result.result).toEqual([])
    })
  })

  describe('createTag', () => {
    it('calls apiPost with correct params', async () => {
      const mockResponse = { code: 0, msg: '', result: { id: '1', name: 'new-tag', type: 'device' } }
      ;(apiPost as ReturnType<typeof vi.fn>).mockResolvedValue(mockResponse)

      await tagApi.createTag('new-tag')

      expect(apiPost).toHaveBeenCalledWith('tags', { name: 'new-tag', type: 'device' })
    })

    it('supports custom type', async () => {
      const mockResponse = { code: 0, msg: '', result: { id: '2', name: 'alarm-tag', type: 'alarm' } }
      ;(apiPost as ReturnType<typeof vi.fn>).mockResolvedValue(mockResponse)

      await tagApi.createTag('alarm-tag', 'alarm')

      expect(apiPost).toHaveBeenCalledWith('tags', { name: 'alarm-tag', type: 'alarm' })
    })
  })

  describe('bindTag', () => {
    it('calls apiPost with tag and target ids', async () => {
      const mockResponse = { code: 0, msg: '', result: { id: 'binding-1' } }
      ;(apiPost as ReturnType<typeof vi.fn>).mockResolvedValue(mockResponse)

      await tagApi.bindTag('tag-1', 'device-1')

      expect(apiPost).toHaveBeenCalledWith('tags/bindings', { tagId: 'tag-1', targetId: 'device-1', targetType: 'device' })
    })
  })

  describe('unbindTag', () => {
    it('calls apiDelete with correct query params', async () => {
      const mockResponse = { code: 0, msg: '', result: null }
      ;(apiDelete as ReturnType<typeof vi.fn>).mockResolvedValue(mockResponse)

      await tagApi.unbindTag('tag-1', 'device-1')

      expect(apiDelete).toHaveBeenCalledWith('tags/bindings?tag_id=tag-1&target_id=device-1')
    })
  })
})
