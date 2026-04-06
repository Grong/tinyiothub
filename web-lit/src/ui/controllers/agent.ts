// web-lit/src/ui/controllers/agent.ts
import type { AppViewState } from '../app-view-state'
import type { ChatMessage, A2uiMessage, SseEvent } from '../types'
import { apiPost } from '../api-client'
import { API_PREFIX } from '../../lib/config'

function getAuthToken(): string | null {
  return sessionStorage.getItem('auth-token')
}

function buildUrl(endpoint: string): string {
  return `${API_PREFIX}/${endpoint.replace(/^\//, '')}`
}

export async function sendAgentMessage(
  host: AppViewState,
  message: string
): Promise<void> {
  const userMsg: ChatMessage = {
    id: crypto.randomUUID(),
    role: 'user',
    content: message,
    timestamp: new Date().toISOString(),
  }
  host.chatMessages = [...host.chatMessages, userMsg]
  host.isStreaming = true
  host.streamingContent = ''

  const assistantMsg: ChatMessage = {
    id: crypto.randomUUID(),
    role: 'assistant',
    content: '',
    timestamp: new Date().toISOString(),
    isStreaming: true,
  }
  host.chatMessages = [...host.chatMessages, assistantMsg]

  try {
    const token = getAuthToken()
    const response = await fetch(buildUrl('agent/chat'), {
      method: 'POST',
      headers: {
        'Content-Type': 'application/json',
        ...(token ? { Authorization: `Bearer ${token}` } : {}),
      },
      body: JSON.stringify({
        message,
        session_id: host.sessionId,
      }),
    })

    if (!response.ok) {
      throw new Error(`Agent request failed: ${response.status}`)
    }

    const reader = response.body?.getReader()
    if (!reader) throw new Error('No response body')

    const decoder = new TextDecoder()
    let buffer = ''

    while (true) {
      const { done, value } = await reader.read()
      if (done) break

      buffer += decoder.decode(value, { stream: true })
      const lines = buffer.split('\n')
      buffer = lines.pop() || ''

      for (const line of lines) {
        if (!line.startsWith('data: ')) continue
        const data = line.slice(6)
        if (!data.trim()) continue

        try {
          const event = JSON.parse(data) as SseEvent
          switch (event.type) {
            case 'delta':
              host.streamingContent += event.content
              assistantMsg.content = host.streamingContent
              host.chatMessages = [...host.chatMessages]
              break
            case 'a2ui':
              handleA2uiMessage(host, assistantMsg, event.message)
              break
            case 'final':
              assistantMsg.content = event.content || host.streamingContent
              assistantMsg.isStreaming = false
              host.chatMessages = [...host.chatMessages]
              break
          }
        } catch {
          // Skip malformed JSON lines
        }
      }
    }
  } catch (error) {
    assistantMsg.content = `Error: ${error instanceof Error ? error.message : 'Unknown error'}`
    assistantMsg.isStreaming = false
    host.chatMessages = [...host.chatMessages]
  } finally {
    host.isStreaming = false
    host.streamingContent = ''
  }
}

function handleA2uiMessage(host: AppViewState, message: ChatMessage, a2uiMsg: A2uiMessage): void {
  if (!message.surfaces) {
    message.surfaces = new Map()
  }
  switch (a2uiMsg.type) {
    case 'createSurface':
      message.surfaces.set(a2uiMsg.surface.surfaceId, a2uiMsg.surface)
      break
    case 'updateComponents':
      for (const surface of message.surfaces.values()) {
        for (const comp of a2uiMsg.components) {
          if (surface.components.some(c => c.id === comp.id)) {
            surface.components = surface.components.map(c => c.id === comp.id ? comp : c)
          } else {
            surface.components = [...surface.components, comp]
          }
        }
      }
      break
    case 'updateDataModel':
      for (const surface of message.surfaces.values()) {
        surface.dataModel = { ...surface.dataModel, ...a2uiMsg.dataModel }
      }
      break
    case 'deleteSurface':
      message.surfaces.delete(a2uiMsg.surfaceId)
      break
  }
  host.chatMessages = [...host.chatMessages]
}

export async function sendAgentAction(
  host: AppViewState,
  componentId: string,
  eventType: string,
  payload: Record<string, unknown>
): Promise<void> {
  await apiPost('agent/action', {
    session_id: host.sessionId,
    component_id: componentId,
    event_type: eventType,
    ...payload,
  })
}

export function clearChat(host: AppViewState): void {
  host.chatMessages = []
  host.sessionId = null
  host.streamingContent = ''
  host.isStreaming = false
}
