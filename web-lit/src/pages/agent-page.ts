/**
 * Agent page - AI chat interface with A2UI rendering
 */

import { LitElement, html} from 'lit'
import { customElement, state } from 'lit/decorators.js'
import {
  $chatMessages,
  $streamingContent,
  $isStreaming,
  $sessionId,
  addMessage,
  appendStreamDelta,
  finalizeStream,
  addA2uiToLastMessage,
  loadMessagesFromStorage,
  saveMessagesToStorage,
} from '../stores/agent-store'
import { sendAgentMessage } from '../services/agent'
import type { A2uiMessage } from '../types/agent-types'
import '../components/agent/chat-thread'
import '../components/agent/chat-input'

@customElement('agent-page')
export class AgentPage extends LitElement {
  createRenderRoot() { return this }
  @state() private messages = $chatMessages.get()
  @state() private streamingContent = $streamingContent.get()
  @state() private isStreaming = $isStreaming.get()
  private _abortController: AbortController | null = null
  private _unsubs: (() => void)[] = []

  

  firstUpdated() {
    loadMessagesFromStorage()
  }

  connectedCallback() {
    super.connectedCallback()
    this._unsubs.push(
      $chatMessages.subscribe(() => {
        this.messages = $chatMessages.get()
        saveMessagesToStorage()
      }),
      $streamingContent.subscribe(() => {
        this.streamingContent = $streamingContent.get()
      }),
      $isStreaming.subscribe(() => {
        this.isStreaming = $isStreaming.get()
      })
    )
  }

  disconnectedCallback() {
    super.disconnectedCallback()
    this._unsubs.forEach(u => u())
    this._unsubs = []
    this._abortController?.abort()
  }

  /**
   * Core send logic — called by both _handleSend (from chat-input event)
   * and _handleSuggestion (from welcome-state button click).
   */
  private async _sendMessage(message: string) {
    if (this.isStreaming) return
    addMessage({ role: 'user', content: message })
    addMessage({ role: 'assistant', content: '', isStreaming: true })
    $isStreaming.set(true)

    this._abortController = new AbortController()

    const onDelta = (content: string) => appendStreamDelta(content)
    const onA2ui = (msg: A2uiMessage) => {
      const surfaceId = (msg.payload as Record<string, unknown>)?.surfaceId as string || 'default'
      addA2uiToLastMessage(surfaceId, msg)
    }
    const onFinal = (content: string) => {
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

    try {
      await sendAgentMessage(
        message,
        $sessionId.get(),
        onDelta,
        onA2ui,
        onFinal,
        this._abortController.signal
      )
    } catch (err: unknown) {
      if (err instanceof DOMException && err.name === 'AbortError') {
        finalizeStream()
        return
      }
      const errorMessage = err instanceof Error ? err.message : String(err)
      const messages = $chatMessages.get()
      const last = messages[messages.length - 1]
      if (last && last.isStreaming) {
        last.content = `连接失败：${errorMessage || '请重试'}`
        last.isStreaming = false
        $chatMessages.set([...messages])
      }
      $streamingContent.set('')
      $isStreaming.set(false)
    }
  }

  private _handleSend(e: CustomEvent) {
    const message = e.detail.message
    this._sendMessage(message)
  }

  private _handleStop() {
    this._abortController?.abort()
  }

  private _handleSuggestion(text: string) {
    this._sendMessage(text)
  }

  render() {
    const hasMessages = this.messages.length > 0

    return html`
      <div class="header">
        <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.5">
          <path d="M9.813 15.904L9 18.75l-.813-2.846a4.5 4.5 0 00-3.09-3.09L2.25 12l2.846-.813a4.5 4.5 0 003.09-3.09L9 5.25l.813 2.846a4.5 4.5 0 003.09 3.09L15.75 12l-2.846.813a4.5 4.5 0 00-3.09 3.09zM18.259 8.715L18 9.75l-.259-1.035a3.375 3.375 0 00-2.455-2.456L14.25 6l1.036-.259a3.375 3.375 0 002.455-2.456L18 2.25l.259 1.035a3.375 3.375 0 002.455 2.456L21.75 6l-1.036.259a3.375 3.375 0 00-2.455 2.456z"/>
        </svg>
        AI 助手
      </div>
      <div class="main">
        ${hasMessages ? html`
          <chat-thread
            .messages="${this.messages}"
            .streamingContent="${this.streamingContent}"
            .isStreaming="${this.isStreaming}"
          ></chat-thread>
        ` : html`
          <div class="welcome">
            <div class="welcome-icon">
              <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.5">
                <path d="M9.813 15.904L9 18.75l-.813-2.846a4.5 4.5 0 00-3.09-3.09L2.25 12l2.846-.813a4.5 4.5 0 003.09-3.09L9 5.25l.813 2.846a4.5 4.5 0 003.09 3.09L15.75 12l-2.846.813a4.5 4.5 0 00-3.09 3.09z"/>
              </svg>
            </div>
            <h2>IoT AI 助手</h2>
            <p>查询设备状态、分析告警、执行命令</p>
            <div class="suggestions">
              <button class="suggestion" @click="${() => this._handleSuggestion('查看所有在线设备')}">查看所有在线设备</button>
              <button class="suggestion" @click="${() => this._handleSuggestion('最近的告警')}">最近的告警</button>
              <button class="suggestion" @click="${() => this._handleSuggestion('系统监控概览')}">系统监控概览</button>
              <button class="suggestion" @click="${() => this._handleSuggestion('设备 XYZ 状态')}">设备状态查询</button>
            </div>
          </div>
        `}
        <chat-input
          ?isStreaming="${this.isStreaming}"
          @message-send="${this._handleSend}"
          @message-stop="${this._handleStop}"
        ></chat-input>
      </div>
    `
  }
}

declare global {
  interface HTMLElementTagNameMap {
    'agent-page': AgentPage
  }
}
