/**
 * Agent chat state management
 */

import { atom } from 'nanostores'
import type { ChatMessage, A2uiMessage, A2uiSurfaceState } from '../types/agent-types'

export const $chatMessages = atom<ChatMessage[]>([])
export const $streamingContent = atom<string>('')
export const $isStreaming = atom<boolean>(false)
export const $sessionId = atom<string>('')

function generateId(): string {
  return crypto.randomUUID()
}

function generateSessionId(): string {
  const stored = sessionStorage.getItem('agent-session-id')
  if (stored) return stored
  const id = `sess-${Date.now()}-${Math.random().toString(36).slice(2, 8)}`
  sessionStorage.setItem('agent-session-id', id)
  return id
}

// Initialize session ID from storage
$sessionId.set(generateSessionId())

export function addMessage(msg: Omit<ChatMessage, 'id' | 'timestamp'>): ChatMessage {
  const full: ChatMessage = {
    ...msg,
    id: generateId(),
    timestamp: Date.now(),
  }
  $chatMessages.set([...$chatMessages.get(), full])
  return full
}

export function appendStreamDelta(delta: string): void {
  $streamingContent.set($streamingContent.get() + delta)
}

export function finalizeStream(): void {
  const content = $streamingContent.get()
  const messages = $chatMessages.get()
  const last = messages[messages.length - 1]
  if (last && last.isStreaming) {
    last.content = content
    last.isStreaming = false
    $chatMessages.set([...messages])
  }
  $streamingContent.set('')
  $isStreaming.set(false)
}

export function clearChat(): void {
  $chatMessages.set([])
  $streamingContent.set('')
  $isStreaming.set(false)
  sessionStorage.removeItem('agent-session-id')
  $sessionId.set(generateSessionId())
}

export function addA2uiToLastMessage(surfaceId: string, a2uiMsg: A2uiMessage): void {
  const messages = $chatMessages.get()
  const last = messages[messages.length - 1]
  if (!last || last.role !== 'assistant') return

  if (!last.surfaces) {
    last.surfaces = new Map()
  }

  let surface = last.surfaces.get(surfaceId)
  if (!surface) {
    surface = { surfaceId, components: [], dataModel: {} }
    last.surfaces.set(surfaceId, surface)
  }

  switch (a2uiMsg.type) {
    case 'createSurface': {
      if (a2uiMsg.payload.title) surface.title = a2uiMsg.payload.title
      break
    }
    case 'updateComponents': {
      const incoming = a2uiMsg.payload.components
      const map = new Map(surface.components.map(c => [c.id, c]))
      for (const comp of incoming) {
        map.set(comp.id, comp)
      }
      surface.components = Array.from(map.values())
      break
    }
    case 'updateDataModel': {
      surface.dataModel = { ...surface.dataModel, ...a2uiMsg.payload }
      break
    }
    case 'deleteSurface': {
      surface.components = []
      break
    }
  }

  $chatMessages.set([...messages])
}

/** Load messages from sessionStorage */
export function loadMessagesFromStorage(): void {
  const stored = sessionStorage.getItem('agent-chat-messages')
  if (stored) {
    try {
      const parsed = JSON.parse(stored)
      // Reconstruct Map from serialized surfaces
      const messages: ChatMessage[] = parsed.map((m: any) => ({
        ...m,
        surfaces: m.surfaces
          ? new Map(Object.entries(m.surfaces)) as Map<string, A2uiSurfaceState>
          : undefined,
      }))
      $chatMessages.set(messages)
    } catch {
      // corrupted data, ignore
    }
  }
}

/** Save messages to sessionStorage */
export function saveMessagesToStorage(): void {
  const messages = $chatMessages.get()
  // Convert Map to plain object for serialization
  const serializable = messages.map(m => ({
    ...m,
    surfaces: m.surfaces ? Object.fromEntries(m.surfaces) : undefined,
  }))
  sessionStorage.setItem('agent-chat-messages', JSON.stringify(serializable))
}
