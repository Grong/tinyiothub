import { describe, it, expect, beforeEach } from 'vitest'
import {
  $chatMessages,
  $isStreaming,
  addMessage,
  appendStreamDelta,
  $streamingContent,
  finalizeStream,
  clearChat,
  addA2uiToLastMessage,
} from './agent-store'

describe('agent-store', () => {
  beforeEach(() => {
    clearChat()
  })

  it('addMessage adds a user message', () => {
    const msg = addMessage({ role: 'user', content: 'hello' })
    expect(msg.role).toBe('user')
    expect(msg.content).toBe('hello')
    expect($chatMessages.get()).toHaveLength(1)
  })

  it('appendStreamDelta accumulates content', () => {
    appendStreamDelta('Hello ')
    appendStreamDelta('world')
    expect($streamingContent.get()).toBe('Hello world')
  })

  it('finalizeStream moves streaming content to last message', () => {
    addMessage({ role: 'assistant', content: '', isStreaming: true })
    appendStreamDelta('streamed text')
    finalizeStream()
    const msgs = $chatMessages.get()
    expect(msgs[0].content).toBe('streamed text')
    expect(msgs[0].isStreaming).toBe(false)
    expect($streamingContent.get()).toBe('')
    expect($isStreaming.get()).toBe(false)
  })

  it('addA2uiToLastMessage creates surface on assistant message', () => {
    addMessage({ role: 'assistant', content: 'test', isStreaming: false })
    addA2uiToLastMessage('surf-1', {
      type: 'createSurface',
      payload: { surfaceId: 'surf-1', title: 'Test' },
    })
    const msgs = $chatMessages.get()
    const surface = msgs[0].surfaces?.get('surf-1')
    expect(surface).toBeDefined()
    expect(surface?.title).toBe('Test')
  })

  it('addA2uiToLastMessage merges components', () => {
    addMessage({ role: 'assistant', content: 'test' })
    addA2uiToLastMessage('surf-1', {
      type: 'updateComponents',
      payload: {
        surfaceId: 'surf-1',
        components: [
          { id: 'c1', type: 'Text', props: { text: 'hello' } },
        ],
      },
    })
    addA2uiToLastMessage('surf-1', {
      type: 'updateComponents',
      payload: {
        surfaceId: 'surf-1',
        components: [
          { id: 'c1', type: 'Text', props: { text: 'updated' } },
          { id: 'c2', type: 'Button', props: { label: 'click' } },
        ],
      },
    })
    const msgs = $chatMessages.get()
    const surface = msgs[0].surfaces?.get('surf-1')
    expect(surface?.components).toHaveLength(2)
    expect(surface?.components[0].props?.text).toBe('updated')
  })
})
