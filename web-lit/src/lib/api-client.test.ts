import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest'

// Mock config before importing api-client
vi.mock('./config', () => ({
  API_PREFIX: '/api/v1',
}))

// We need to mock fetch
const mockFetch = vi.fn()
vi.stubGlobal('fetch', mockFetch)

// Import after mocking
import { ApiError } from './api-client'
import { apiGet, apiPost, apiPut, apiDelete } from './api-client'

describe('apiGet error handling', () => {
  beforeEach(() => {
    vi.clearAllMocks()
  })

  it('throws ApiError when response code is not 0', async () => {
    mockFetch.mockResolvedValueOnce({
      ok: true,
      json: () => Promise.resolve({ code: -1, msg: 'Device not found', result: null }),
    })

    await expect(apiGet('devices/999')).rejects.toThrow(ApiError)
  })

  it('throws ApiError with correct code and message', async () => {
    mockFetch.mockResolvedValueOnce({
      ok: true,
      json: () => Promise.resolve({ code: 403, msg: 'Forbidden', result: null }),
    })

    try {
      await apiGet('devices')
    } catch (e) {
      expect(e).toBeInstanceOf(ApiError)
      expect((e as ApiError).code).toBe(403)
      expect((e as ApiError).message).toBe('Forbidden')
    }
  })

  it('throws ApiError when network error occurs', async () => {
    mockFetch.mockRejectedValueOnce(new Error('Network failure'))

    await expect(apiGet('devices')).rejects.toThrow('Network failure')
  })

  it('throws ApiError when response is not ok (HTTP error)', async () => {
    mockFetch.mockResolvedValueOnce({
      ok: false,
      status: 500,
      json: () => Promise.resolve({ code: 500, msg: 'Internal server error', result: null }),
    })

    await expect(apiGet('devices')).rejects.toThrow()
  })
})

describe('apiPost error handling', () => {
  beforeEach(() => {
    vi.clearAllMocks()
  })

  it('throws ApiError when create fails', async () => {
    mockFetch.mockResolvedValueOnce({
      ok: true,
      json: () => Promise.resolve({ code: 400, msg: 'Invalid device data', result: null }),
    })

    await expect(apiPost('devices', { name: '' })).rejects.toThrow(ApiError)
  })

  it('sends body data correctly', async () => {
    mockFetch.mockResolvedValueOnce({
      ok: true,
      json: () => Promise.resolve({ code: 0, msg: '', result: { id: 'new-1' } }),
    })

    await apiPost('devices', { name: 'test-device' })

    expect(mockFetch).toHaveBeenCalledWith(
      expect.stringContaining('/api/v1/devices'),
      expect.objectContaining({
        method: 'POST',
        body: JSON.stringify({ name: 'test-device' }),
      })
    )
  })
})

describe('apiPut error handling', () => {
  beforeEach(() => {
    vi.clearAllMocks()
  })

  it('throws ApiError when update fails', async () => {
    mockFetch.mockResolvedValueOnce({
      ok: true,
      json: () => Promise.resolve({ code: 404, msg: 'Device not found', result: null }),
    })

    await expect(apiPut('devices/999', { name: 'updated' })).rejects.toThrow(ApiError)
  })
})

describe('apiDelete error handling', () => {
  beforeEach(() => {
    vi.clearAllMocks()
  })

  it('throws ApiError when delete fails', async () => {
    mockFetch.mockResolvedValueOnce({
      ok: true,
      json: () => Promise.resolve({ code: 404, msg: 'Device not found', result: null }),
    })

    await expect(apiDelete('devices/999')).rejects.toThrow(ApiError)
  })

  it('sends DELETE request correctly', async () => {
    mockFetch.mockResolvedValueOnce({
      ok: true,
      json: () => Promise.resolve({ code: 0, msg: '', result: true }),
    })

    await apiDelete('devices/123')

    expect(mockFetch).toHaveBeenCalledWith(
      expect.stringContaining('/api/v1/devices/123'),
      expect.objectContaining({ method: 'DELETE' })
    )
  })
})

describe('authentication', () => {
  beforeEach(() => {
    vi.clearAllMocks()
    vi.stubGlobal('sessionStorage', {
      getItem: vi.fn(),
      setItem: vi.fn(),
      removeItem: vi.fn(),
    })
  })

  it('does not add auth header when no token', async () => {
    ;(sessionStorage.getItem as ReturnType<typeof vi.fn>).mockReturnValue(null)

    mockFetch.mockResolvedValueOnce({
      ok: true,
      json: () => Promise.resolve({ code: 0, msg: '', result: null }),
    })

    await apiGet('devices')

    const call = mockFetch.mock.calls[0]
    const headers = call[1].headers as Record<string, string>
    expect(headers['Authorization']).toBeUndefined()
  })

  it('adds auth header when token exists', async () => {
    ;(sessionStorage.getItem as ReturnType<typeof vi.fn>).mockReturnValue('test-token-123')

    mockFetch.mockResolvedValueOnce({
      ok: true,
      json: () => Promise.resolve({ code: 0, msg: '', result: null }),
    })

    await apiGet('devices')

    const call = mockFetch.mock.calls[0]
    const headers = call[1].headers as Record<string, string>
    expect(headers['Authorization']).toBe('Bearer test-token-123')
  })
})
