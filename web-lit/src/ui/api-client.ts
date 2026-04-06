// web-lit/src/ui/api-client.ts
import { keysToCamelCase, keysToSnakeCase, type KeysToCamelCase } from '../lib/case-converter'
import { API_PREFIX } from '../lib/config'

export interface ApiResponse<T> {
  code: number
  msg: string
  result: T | null
}

export interface PaginatedResponse<T> {
  data: T[]
  pagination: {
    page: number
    pageSize: number
    totalPages: number
    totalCount: number
  }
}

export class ApiError extends Error {
  constructor(
    message: string,
    public code: number,
    public data?: unknown,
    public status?: number
  ) {
    super(message)
    this.name = 'ApiError'
  }
}

// Module-level state (set by app-lifecycle on login/workspace change)
let _workspaceId: string | null = null
export function setWorkspaceId(id: string | null) {
  _workspaceId = id
}

function getAuthToken(): string | null {
  return sessionStorage.getItem('auth-token')
}

let refreshPromise: Promise<void> | null = null

async function refreshToken(): Promise<void> {
  if (!refreshPromise) {
    refreshPromise = (async () => {
      try {
        const token = getAuthToken()
        const res = await fetch(`${API_PREFIX}/auth/refresh`, {
          method: 'POST',
          headers: {
            'Content-Type': 'application/json',
            ...(token ? { Authorization: `Bearer ${token}` } : {}),
          },
        })
        if (!res.ok) throw new Error('Refresh failed')
        const data = await res.json()
        if (data.result?.accessToken) {
          sessionStorage.setItem('auth-token', data.result.accessToken)
        }
      } finally {
        refreshPromise = null
      }
    })()
  }
  return refreshPromise
}

async function request<T>(
  method: string,
  path: string,
  options: {
    body?: unknown
    params?: Record<string, unknown>
    headers?: Record<string, string>
    skipAuth?: boolean
  } = {}
): Promise<ApiResponse<T>> {
  const { body, params, headers = {}, skipAuth } = options

  let url = `${API_PREFIX}/${path.replace(/^\//, '')}`
  if (params) {
    const snakeParams = keysToSnakeCase(params)
    const searchParams = new URLSearchParams()
    for (const [key, value] of Object.entries(snakeParams)) {
      if (value != null) searchParams.append(key, String(value))
    }
    const qs = searchParams.toString()
    if (qs) url += `?${qs}`
  }

  const token = getAuthToken()
  const fetchHeaders: Record<string, string> = {
    'Content-Type': 'application/json',
    ...headers,
  }
  if (token && !skipAuth) {
    fetchHeaders['Authorization'] = `Bearer ${token}`
  }
  if (_workspaceId) {
    fetchHeaders['X-Workspace-Id'] = _workspaceId
  }

  const response = await fetch(url, {
    method,
    headers: fetchHeaders,
    body: body != null ? JSON.stringify(keysToSnakeCase(body)) : undefined,
  })

  if (response.status === 401 && !skipAuth) {
    try {
      await refreshToken()
      fetchHeaders['Authorization'] = `Bearer ${getAuthToken()}`
      const retryResponse = await fetch(url, {
        method,
        headers: fetchHeaders,
        body: body != null ? JSON.stringify(keysToSnakeCase(body)) : undefined,
      })
      if (!retryResponse.ok) {
        if (retryResponse.status === 401) {
          sessionStorage.removeItem('auth-token')
          window.dispatchEvent(new CustomEvent('auth-error'))
        }
        throw new ApiError('Unauthorized', -1, null, retryResponse.status)
      }
      const retryData = await retryResponse.json()
      return { ...retryData, result: keysToCamelCase(retryData.result) } as ApiResponse<T>
    } catch {
      sessionStorage.removeItem('auth-token')
      window.dispatchEvent(new CustomEvent('auth-error'))
      throw new ApiError('Session expired', -1, null, 401)
    }
  }

  if (!response.ok) {
    const errorData = await response.json().catch(() => ({}))
    throw new ApiError(
      errorData.msg || `HTTP ${response.status}`,
      errorData.code ?? -1,
      errorData.result,
      response.status
    )
  }

  const data = await response.json()
  if (data.code !== 0) {
    throw new ApiError(data.msg || 'API error', data.code, data.result)
  }

  return { ...data, result: keysToCamelCase(data.result) } as ApiResponse<T>
}

export const apiClient = {
  get: <T>(path: string, params?: Record<string, unknown>, headers?: Record<string, string>) =>
    request<T>('GET', path, { params, headers }),

  post: <T>(path: string, body?: unknown, headers?: Record<string, string>) =>
    request<T>('POST', path, { body, headers }),

  put: <T>(path: string, body?: unknown, headers?: Record<string, string>) =>
    request<T>('PUT', path, { body, headers }),

  delete: <T>(path: string, headers?: Record<string, string>) =>
    request<T>('DELETE', path, { headers }),

  patch: <T>(path: string, body?: unknown, headers?: Record<string, string>) =>
    request<T>('PATCH', path, { body, headers }),
}

export const apiGet = apiClient.get
export const apiPost = apiClient.post
export const apiPut = apiClient.put
export const apiDelete = apiClient.delete
export const apiPatch = apiClient.patch
