/**
 * Agent chat SSE service
 * Uses raw fetch + ReadableStream for server-sent events
 */

import type { A2uiMessage } from '../types/agent-types'
import { API_PREFIX } from '../lib/config'

const getAuthToken = (): string | null => {
  if (typeof window === 'undefined') return null
  return sessionStorage.getItem('auth-token')
}

const buildUrl = (endpoint: string): string => {
  const normalizedEndpoint = endpoint.startsWith('/') ? endpoint : `/${endpoint}`
  return `${API_PREFIX}${normalizedEndpoint}`
}

/**
 * Stream an agent chat message via SSE.
 * Posts to `agent/chat` and reads the response body as a ReadableStream,
 * dispatching parsed events to the provided callbacks.
 */
export async function sendAgentMessage(
  message: string,
  sessionId: string,
  onDelta: (content: string) => void,
  onA2ui: (msg: A2uiMessage) => void,
  onFinal: (content: string) => void,
  signal?: AbortSignal
): Promise<void> {
  const url = buildUrl('agent/chat')

  const headers: Record<string, string> = {
    'Content-Type': 'application/json',
    'Accept': 'text/event-stream',
  }

  const token = getAuthToken()
  if (token) {
    headers['Authorization'] = `Bearer ${token}`
  }

  const response = await fetch(url, {
    method: 'POST',
    headers,
    body: JSON.stringify({ message, session_id: sessionId }),
    signal,
  })

  if (!response.ok) {
    const text = await response.text().catch(() => '')
    throw new Error(`Agent chat failed (${response.status}): ${text || response.statusText}`)
  }

  if (!response.body) {
    throw new Error('Response body is null — streaming not supported by this browser')
  }

  const reader = response.body.getReader()
  const decoder = new TextDecoder()
  let buffer = ''

  while (true) {
    const { done, value } = await reader.read()

    if (done) break

    buffer += decoder.decode(value, { stream: true })

    const lines = buffer.split('\n')
    // Keep the last (possibly incomplete) line in the buffer
    buffer = lines.pop() ?? ''

    for (const rawLine of lines) {
      const line = rawLine.trimEnd()
      if (!line) continue

      // Skip SSE comments
      if (line.startsWith(':')) continue

      // Skip "event:" lines (e.g. "event: done")
      if (line.startsWith('event:')) continue

      // Only process "data:" lines
      if (!line.startsWith('data:')) continue

      const data = line.slice(5).trimStart()
      if (!data) continue

      try {
        const parsed = JSON.parse(data) as { type: string; content?: string; message?: A2uiMessage }

        switch (parsed.type) {
          case 'delta':
            if (typeof parsed.content === 'string') {
              onDelta(parsed.content)
            }
            break
          case 'a2ui':
            if (parsed.message) {
              onA2ui(parsed.message)
            }
            break
          case 'final':
            if (typeof parsed.content === 'string') {
              onFinal(parsed.content)
            }
            break
          default:
            // Unknown event type — ignore silently
            break
        }
      } catch {
        // Malformed JSON — skip this line
        console.warn('[agent-sse] Failed to parse SSE data:', data)
      }
    }
  }

  // Process any remaining data in the buffer
  if (buffer.trim()) {
    const line = buffer.trimEnd()
    if (line.startsWith('data:')) {
      const data = line.slice(5).trimStart()
      if (data) {
        try {
          const parsed = JSON.parse(data) as { type: string; content?: string; message?: A2uiMessage }
          if (parsed.type === 'final' && typeof parsed.content === 'string') {
            onFinal(parsed.content)
          }
        } catch {
          // Ignore final buffer parse errors
        }
      }
    }
  }
}

/**
 * POST an A2UI action event back to the backend.
 * Called when the user interacts with agent-rendered UI components.
 */
export async function sendAgentAction(
  sessionId: string,
  componentId: string,
  eventType: string,
  payload: unknown
): Promise<void> {
  const url = buildUrl('agent/action')

  const headers: Record<string, string> = {
    'Content-Type': 'application/json',
  }

  const token = getAuthToken()
  if (token) {
    headers['Authorization'] = `Bearer ${token}`
  }

  const response = await fetch(url, {
    method: 'POST',
    headers,
    body: JSON.stringify({
      session_id: sessionId,
      component_id: componentId,
      event_type: eventType,
      payload,
    }),
  })

  if (!response.ok) {
    const text = await response.text().catch(() => '')
    throw new Error(`Agent action failed (${response.status}): ${text || response.statusText}`)
  }
}
