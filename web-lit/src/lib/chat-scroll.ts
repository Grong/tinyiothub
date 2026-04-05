/**
 * Chat auto-scroll manager
 * Adapted from OpenClaw ui/src/ui/app-scroll.ts
 */

const NEAR_BOTTOM_THRESHOLD = 450

export interface ChatScrollHost {
  updateComplete: Promise<unknown>
  querySelector: (selectors: string) => Element | null
  chatScrollFrame: number | null
  chatScrollTimeout: number | null
  chatHasAutoScrolled: boolean
  chatUserNearBottom: boolean
  chatNewMessagesBelow: boolean
}

function pickScrollTarget(host: ChatScrollHost): HTMLElement | null {
  const container = host.querySelector('.chat-thread') as HTMLElement | null
  if (container) {
    const overflowY = getComputedStyle(container).overflowY
    const canScroll =
      overflowY === 'auto' ||
      overflowY === 'scroll' ||
      container.scrollHeight - container.clientHeight > 1
    if (canScroll) return container
  }
  return (document.scrollingElement ?? document.documentElement) as HTMLElement | null
}

export function scheduleChatScroll(host: ChatScrollHost, force = false, smooth = false) {
  if (host.chatScrollFrame) cancelAnimationFrame(host.chatScrollFrame)
  if (host.chatScrollTimeout != null) {
    clearTimeout(host.chatScrollTimeout)
    host.chatScrollTimeout = null
  }

  void host.updateComplete.then(() => {
    host.chatScrollFrame = requestAnimationFrame(() => {
      host.chatScrollFrame = null
      const target = pickScrollTarget(host)
      if (!target) return

      const distanceFromBottom = target.scrollHeight - target.scrollTop - target.clientHeight
      const effectiveForce = force && !host.chatHasAutoScrolled
      const shouldStick =
        effectiveForce || host.chatUserNearBottom || distanceFromBottom < NEAR_BOTTOM_THRESHOLD

      if (!shouldStick) {
        host.chatNewMessagesBelow = true
        return
      }
      if (effectiveForce) host.chatHasAutoScrolled = true

      const smoothEnabled =
        smooth &&
        (typeof window === 'undefined' ||
          typeof window.matchMedia !== 'function' ||
          !window.matchMedia('(prefers-reduced-motion: reduce)').matches)

      if (typeof target.scrollTo === 'function') {
        target.scrollTo({ top: target.scrollHeight, behavior: smoothEnabled ? 'smooth' : 'auto' })
      } else {
        target.scrollTop = target.scrollHeight
      }
      host.chatUserNearBottom = true
      host.chatNewMessagesBelow = false

      const retryDelay = effectiveForce ? 150 : 120
      host.chatScrollTimeout = window.setTimeout(() => {
        host.chatScrollTimeout = null
        const latest = pickScrollTarget(host)
        if (!latest) return
        const latestDistance = latest.scrollHeight - latest.scrollTop - latest.clientHeight
        const shouldRetry = effectiveForce || host.chatUserNearBottom || latestDistance < NEAR_BOTTOM_THRESHOLD
        if (!shouldRetry) return
        latest.scrollTop = latest.scrollHeight
        host.chatUserNearBottom = true
      }, retryDelay)
    })
  })
}

export function handleChatScroll(host: ChatScrollHost, event: Event) {
  const container = event.currentTarget as HTMLElement | null
  if (!container) return
  const distanceFromBottom = container.scrollHeight - container.scrollTop - container.clientHeight
  host.chatUserNearBottom = distanceFromBottom < NEAR_BOTTOM_THRESHOLD
  if (host.chatUserNearBottom) {
    host.chatNewMessagesBelow = false
  }
}

export function resetChatScroll(host: ChatScrollHost) {
  host.chatHasAutoScrolled = false
  host.chatUserNearBottom = true
  host.chatNewMessagesBelow = false
}
