# AI Agent Page (A2UI) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build an IoT-enhanced AI chat page for web-lit with SSE streaming, Markdown rendering, and A2UI protocol-driven dynamic component rendering.

**Architecture:** Lit 3 Web Components page at `/agent` route. SSE streaming via raw `fetch` + `ReadableStream`. A2UI rendering engine with Catalog Registry (type→component mapping), Component Factory (`<a2ui-component>`), and Surface Manager (`<a2ui-surface>`). Nanostore for state. sessionStorage for session persistence.

**Tech Stack:** Lit 3, Vite, nanostores, marked + DOMPurify, TypeScript, SSE

---

## File Structure

```
web-lit/src/
├── types/agent-types.ts                    # A2UI + chat type definitions
├── stores/agent-store.ts                   # Chat state (nanostore atoms)
├── services/agent.ts                       # SSE fetch + stream parsing
├── lib/markdown.ts                         # Markdown renderer (from OpenClaw)
├── lib/chat-scroll.ts                      # Auto-scroll manager (from OpenClaw)
├── components/agent/
│   ├── chat-input.ts                       # Textarea + send/stop button
│   ├── chat-thread.ts                      # Scrollable message list
│   ├── message-group.ts                    # User/assistant bubble renderer
│   ├── streaming-message.ts                # Live streaming indicator
│   └── a2ui/
│       ├── a2ui-surface.ts                 # Surface lifecycle manager
│       ├── a2ui-component.ts               # Component factory (dynamic render)
│       └── catalog/
│           ├── index.ts                    # Registry (type→class map)
│           ├── basic/
│           │   ├── a2ui-text.ts
│           │   ├── a2ui-row.ts
│           │   ├── a2ui-column.ts
│           │   ├── a2ui-card.ts
│           │   ├── a2ui-button.ts
│           │   └── a2ui-divider.ts
│           ├── device-card.ts
│           ├── device-table.ts
│           ├── data-chart.ts
│           ├── control-panel.ts
│           ├── confirmation-dialog.ts
│           ├── progress-indicator.ts
│           └── real-time-toggle.ts
├── pages/agent-page.ts                     # Page entry + welcome state
```

Existing files to modify:
- `app.ts` — add route + import
- `app-sidebar.ts` — add nav item

---

### Task 1: Install dependencies

**Files:**
- Modify: `web-lit/package.json`

- [ ] **Step 1: Install marked and DOMPurify**

```bash
cd web-lit && npm install marked@^17.0.5 dompurify@^3.3.3 && npm install -D @types/dompurify@^3.0.5
```

- [ ] **Step 2: Verify installation**

```bash
cd web-lit && npm ls marked dompurify @types/dompurify
```

Expected: all three listed without errors.

- [ ] **Step 3: Verify build still works**

```bash
cd web-lit && npx vite build 2>&1 | tail -5
```

Expected: build succeeds (no new errors).

- [ ] **Step 4: Commit**

```bash
git add web-lit/package.json web-lit/package-lock.json
git commit -m "chore(web-lit): add marked, dompurify, @types/dompurify"
```

---

### Task 2: Create type definitions

**Files:**
- Create: `web-lit/src/types/agent-types.ts`

- [ ] **Step 1: Write agent-types.ts**

```typescript
/**
 * Agent chat + A2UI type definitions
 */

/** A2UI message types (v0.10 protocol) */
export type A2uiMessage =
  | { type: 'createSurface'; payload: { surfaceId: string; title?: string; layout?: string } }
  | { type: 'updateComponents'; payload: { surfaceId: string; components: A2uiComponentDescriptor[] } }
  | { type: 'updateDataModel'; payload: Record<string, unknown> }
  | { type: 'deleteSurface'; payload: { surfaceId: string } }
  | { type: 'callFunction'; payload: { functionId: string; args: Record<string, unknown> } }
  | { type: 'actionResponse'; payload: { actionId: string; result: unknown } }

/** A2UI component descriptor */
export interface A2uiComponentDescriptor {
  id: string
  type: string
  props?: Record<string, unknown>
  children?: A2uiComponentDescriptor[]
}

/** A2UI surface state */
export interface A2uiSurfaceState {
  surfaceId: string
  title?: string
  components: A2uiComponentDescriptor[]
  dataModel: Record<string, unknown>
}

/** Chat message */
export interface ChatMessage {
  id: string
  role: 'user' | 'assistant'
  content: string
  timestamp: number
  surfaces?: Map<string, A2uiSurfaceState>
  isStreaming?: boolean
}

/** SSE event types from the agent endpoint */
export type SseEvent =
  | { type: 'delta'; content: string }
  | { type: 'a2ui'; message: A2uiMessage }
  | { type: 'final'; content: string }

/** Device property for IoT components */
export interface DeviceProperty {
  name: string
  displayName?: string
  value: string
  unit?: string
  currentValue?: string
  dataType?: string
}
```

- [ ] **Step 2: Verify TypeScript compiles**

```bash
cd web-lit && npx tsc --noEmit src/types/agent-types.ts 2>&1
```

Expected: no errors.

- [ ] **Step 3: Commit**

```bash
git add web-lit/src/types/agent-types.ts
git commit -m "feat(web-lit): add agent chat + A2UI type definitions"
```

---

### Task 3: Create agent store

**Files:**
- Create: `web-lit/src/stores/agent-store.ts`

- [ ] **Step 1: Write agent-store.ts**

```typescript
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
```

- [ ] **Step 2: Write unit test**

```typescript
// web-lit/src/stores/agent-store.test.ts
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
```

- [ ] **Step 3: Run tests**

```bash
cd web-lit && npx vitest run src/stores/agent-store.test.ts
```

Expected: all tests pass.

- [ ] **Step 4: Commit**

```bash
git add web-lit/src/stores/agent-store.ts web-lit/src/stores/agent-store.test.ts
git commit -m "feat(web-lit): add agent chat store with A2UI surface management"
```

---

### Task 4: Create markdown renderer

**Files:**
- Create: `web-lit/src/lib/markdown.ts`

- [ ] **Step 1: Write markdown.ts**

Adapted from OpenClaw `ui/src/ui/markdown.ts`. Removed OpenClaw-specific imports (`truncateText` from `./format.ts`). Simplified for web-lit use case.

```typescript
/**
 * Markdown rendering with sanitization
 * Adapted from OpenClaw ui/src/ui/markdown.ts
 */

import DOMPurify from 'dompurify'
import { marked } from 'marked'

const allowedTags = [
  'a', 'b', 'blockquote', 'br', 'button', 'code', 'del', 'details', 'div',
  'em', 'h1', 'h2', 'h3', 'h4', 'hr', 'i', 'li', 'ol', 'p', 'pre', 'span',
  'strong', 'summary', 'table', 'tbody', 'td', 'th', 'thead', 'tr', 'ul', 'img',
]

const allowedAttrs = [
  'class', 'href', 'rel', 'target', 'title', 'start', 'src', 'alt',
  'data-code', 'type', 'aria-label',
]

const sanitizeOptions = {
  ALLOWED_TAGS: allowedTags,
  ALLOWED_ATTR: allowedAttrs,
  ADD_DATA_URI_TAGS: ['img'],
}

let hooksInstalled = false
const MARKDOWN_CHAR_LIMIT = 140_000
const MARKDOWN_PARSE_LIMIT = 40_000
const markdownCache = new Map<string, string>()
const MARKDOWN_CACHE_LIMIT = 200
const MARKDOWN_CACHE_MAX_CHARS = 50_000
const INLINE_DATA_IMAGE_RE = /^data:image\/[a-z0-9.+-]+;base64,/i

function getCachedMarkdown(key: string): string | null {
  const cached = markdownCache.get(key)
  if (cached === undefined) return null
  markdownCache.delete(key)
  markdownCache.set(key, cached)
  return cached
}

function setCachedMarkdown(key: string, value: string) {
  markdownCache.set(key, value)
  if (markdownCache.size > MARKDOWN_CACHE_LIMIT) {
    const oldest = markdownCache.keys().next().value
    if (oldest) markdownCache.delete(oldest)
  }
}

function installHooks() {
  if (hooksInstalled) return
  hooksInstalled = true

  DOMPurify.addHook('afterSanitizeAttributes', (node) => {
    if (!(node instanceof HTMLAnchorElement)) return
    const href = node.getAttribute('href')
    if (!href) return
    try {
      const url = new URL(href, window.location.href)
      if (url.protocol !== 'http:' && url.protocol !== 'https:' && url.protocol !== 'mailto:') {
        node.removeAttribute('href')
        return
      }
    } catch {
      // relative URLs are fine
    }
    node.setAttribute('rel', 'noreferrer noopener')
    node.setAttribute('target', '_blank')
  })
}

const htmlEscapeRenderer = new marked.Renderer()
htmlEscapeRenderer.html = ({ text }: { text: string }) => escapeHtml(text)
htmlEscapeRenderer.image = (token: { href?: string | null; text?: string | null }) => {
  const label = token.text?.trim() || 'image'
  const href = token.href?.trim() ?? ''
  if (!INLINE_DATA_IMAGE_RE.test(href)) return escapeHtml(label)
  return `<img class="markdown-inline-image" src="${escapeHtml(href)}" alt="${escapeHtml(label)}">`
}
htmlEscapeRenderer.code = ({ text, lang }: { text: string; lang?: string }) => {
  const langClass = lang ? ` class="language-${escapeHtml(lang)}"` : ''
  const safeText = escapeHtml(text)
  const codeBlock = `<pre><code${langClass}>${safeText}</code></pre>`
  const langLabel = lang ? `<span class="code-block-lang">${escapeHtml(lang)}</span>` : ''
  const attrSafe = text
    .replace(/&/g, '&amp;')
    .replace(/"/g, '&quot;')
    .replace(/</g, '&lt;')
    .replace(/>/g, '&gt;')
  const copyBtn = `<button type="button" class="code-block-copy" data-code="${attrSafe}" aria-label="Copy code"><span class="code-block-copy__idle">Copy</span><span class="code-block-copy__done">Copied!</span></button>`
  const header = `<div class="code-block-header">${langLabel}${copyBtn}</div>`

  const trimmed = text.trim()
  const isJson =
    lang === 'json' ||
    (!lang &&
      ((trimmed.startsWith('{') && trimmed.endsWith('}')) ||
        (trimmed.startsWith('[') && trimmed.endsWith(']'))))
  if (isJson) {
    const lineCount = text.split('\n').length
    const label = lineCount > 1 ? `JSON · ${lineCount} lines` : 'JSON'
    return `<details class="json-collapse"><summary>${label}</summary><div class="code-block-wrapper">${header}${codeBlock}</div></details>`
  }
  return `<div class="code-block-wrapper">${header}${codeBlock}</div>`
}

function escapeHtml(value: string): string {
  return value
    .replace(/&/g, '&amp;')
    .replace(/</g, '&lt;')
    .replace(/>/g, '&gt;')
    .replace(/"/g, '&quot;')
    .replace(/'/g, '&#39;')
}

function renderEscapedPlainTextHtml(value: string): string {
  return `<div class="markdown-plain-text-fallback">${escapeHtml(value.replace(/\r\n?/g, '\n'))}</div>`
}

function truncateText(text: string, limit: number): { text: string; truncated: boolean; total: number } {
  if (text.length <= limit) return { text, truncated: false, total: text.length }
  // Try to break at a line boundary
  const slice = text.slice(0, limit)
  const lastNewline = slice.lastIndexOf('\n')
  const cutoff = lastNewline > limit * 0.8 ? lastNewline : limit
  return { text: text.slice(0, cutoff), truncated: true, total: text.length }
}

export function toSanitizedMarkdownHtml(markdown: string): string {
  const input = markdown.trim()
  if (!input) return ''
  installHooks()
  if (input.length <= MARKDOWN_CACHE_MAX_CHARS) {
    const cached = getCachedMarkdown(input)
    if (cached !== null) return cached
  }
  const truncated = truncateText(input, MARKDOWN_CHAR_LIMIT)
  const suffix = truncated.truncated
    ? `\n\n… truncated (${truncated.total} chars, showing first ${truncated.text.length}).`
    : ''
  if (truncated.text.length > MARKDOWN_PARSE_LIMIT) {
    const html = renderEscapedPlainTextHtml(`${truncated.text}${suffix}`)
    const sanitized = DOMPurify.sanitize(html, sanitizeOptions)
    if (input.length <= MARKDOWN_CACHE_MAX_CHARS) setCachedMarkdown(input, sanitized)
    return sanitized
  }
  let rendered: string
  try {
    rendered = marked.parse(`${truncated.text}${suffix}`, {
      renderer: htmlEscapeRenderer,
      gfm: true,
      breaks: true,
    }) as string
  } catch {
    const escaped = escapeHtml(`${truncated.text}${suffix}`)
    rendered = `<pre class="code-block">${escaped}</pre>`
  }
  const sanitized = DOMPurify.sanitize(rendered, sanitizeOptions)
  if (input.length <= MARKDOWN_CACHE_MAX_CHARS) setCachedMarkdown(input, sanitized)
  return sanitized
}
```

- [ ] **Step 2: Write unit test**

```typescript
// web-lit/src/lib/markdown.test.ts
import { describe, it, expect } from 'vitest'
import { toSanitizedMarkdownHtml } from './markdown'

describe('toSanitizedMarkdownHtml', () => {
  it('returns empty string for empty input', () => {
    expect(toSanitizedMarkdownHtml('')).toBe('')
    expect(toSanitizedMarkdownHtml('  ')).toBe('')
  })

  it('renders bold markdown', () => {
    const result = toSanitizedMarkdownHtml('**hello**')
    expect(result).toContain('<strong>hello</strong>')
  })

  it('renders code blocks', () => {
    const result = toSanitizedMarkdownHtml('```js\nconst x = 1\n```')
    expect(result).toContain('<code')
    expect(result).toContain('const x = 1')
  })

  it('sanitizes script tags', () => {
    const result = toSanitizedMarkdownHtml('<script>alert("xss")</script>')
    expect(result).not.toContain('<script')
  })

  it('renders links with target=_blank', () => {
    const result = toSanitizedMarkdownHtml('[click](https://example.com)')
    expect(result).toContain('target="_blank"')
    expect(result).toContain('rel="noreferrer noopener"')
  })

  it('renders tables', () => {
    const md = '| A | B |\n|---|---|\n| 1 | 2 |'
    const result = toSanitizedMarkdownHtml(md)
    expect(result).toContain('<table>')
    expect(result).toContain('<td>1</td>')
  })
})
```

- [ ] **Step 3: Run tests**

```bash
cd web-lit && npx vitest run src/lib/markdown.test.ts
```

Expected: all tests pass.

- [ ] **Step 4: Commit**

```bash
git add web-lit/src/lib/markdown.ts web-lit/src/lib/markdown.test.ts
git commit -m "feat(web-lit): add markdown renderer with DOMPurify sanitization"
```

---

### Task 5: Create chat scroll manager

**Files:**
- Create: `web-lit/src/lib/chat-scroll.ts`

- [ ] **Step 1: Write chat-scroll.ts**

Adapted from OpenClaw `ui/src/ui/app-scroll.ts`. Removed `scheduleLogsScroll`, `handleLogsScroll`, `exportLogs`, `observeTopbar` (not needed). Simplified `ScrollHost` to match web-lit Lit component usage.

```typescript
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
```

- [ ] **Step 2: Verify TypeScript compiles**

```bash
cd web-lit && npx tsc --noEmit src/lib/chat-scroll.ts 2>&1
```

Expected: no errors.

- [ ] **Step 3: Commit**

```bash
git add web-lit/src/lib/chat-scroll.ts
git commit -m "feat(web-lit): add chat auto-scroll manager"
```

---

### Task 6: Create SSE service

**Files:**
- Create: `web-lit/src/services/agent.ts`

- [ ] **Step 1: Write agent.ts**

```typescript
/**
 * Agent chat SSE service
 * Uses raw fetch + ReadableStream for server-sent events
 */

import type { A2uiMessage, SseEvent } from '../types/agent-types'
import { API_PREFIX } from '../lib/config'

const getAuthToken = (): string | null => {
  if (typeof window === 'undefined') return null
  return sessionStorage.getItem('auth-token')
}

const buildUrl = (endpoint: string): string => {
  const normalizedEndpoint = endpoint.startsWith('/') ? endpoint : `/${endpoint}`
  return `${API_PREFIX}${normalizedEndpoint}`
}

export async function sendAgentMessage(
  message: string,
  sessionId: string,
  onDelta: (content: string) => void,
  onA2ui: (msg: A2uiMessage) => void,
  onFinal: (content: string) => void,
  signal?: AbortSignal
): Promise<void> {
  const url = buildUrl('agent/chat')
  const token = getAuthToken()

  const headers: Record<string, string> = {
    'Content-Type': 'application/json',
  }
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
    throw new Error(text || `Agent chat failed: ${response.status}`)
  }

  if (!response.body) {
    throw new Error('No response body for SSE stream')
  }

  const reader = response.body.getReader()
  const decoder = new TextDecoder()
  let buffer = ''

  try {
    while (true) {
      const { done, value } = await reader.read()
      if (done) break

      buffer += decoder.decode(value, { stream: true })
      const lines = buffer.split('\n')
      buffer = lines.pop() ?? ''

      for (const line of lines) {
        const trimmed = line.trim()
        if (!trimmed || !trimmed.startsWith('data:')) continue

        const dataStr = trimmed.slice(5).trim()
        if (!dataStr) continue

        try {
          const event = JSON.parse(dataStr) as SseEvent
          switch (event.type) {
            case 'delta':
              onDelta(event.content)
              break
            case 'a2ui':
              onA2ui(event.message)
              break
            case 'final':
              onFinal(event.content)
              break
          }
        } catch {
          // Skip malformed JSON lines
        }
      }
    }
  } finally {
    reader.releaseLock()
  }
}

export async function sendAgentAction(
  sessionId: string,
  componentId: string,
  eventType: string,
  payload: unknown
): Promise<void> {
  const url = buildUrl('agent/action')
  const token = getAuthToken()

  const headers: Record<string, string> = {
    'Content-Type': 'application/json',
  }
  if (token) {
    headers['Authorization'] = `Bearer ${token}`
  }

  await fetch(url, {
    method: 'POST',
    headers,
    body: JSON.stringify({
      session_id: sessionId,
      component_id: componentId,
      event_type: eventType,
      payload,
    }),
  })
}
```

- [ ] **Step 2: Verify TypeScript compiles**

```bash
cd web-lit && npx tsc --noEmit src/services/agent.ts 2>&1
```

Expected: no errors.

- [ ] **Step 3: Commit**

```bash
git add web-lit/src/services/agent.ts
git commit -m "feat(web-lit): add agent SSE service with ReadableStream parsing"
```

---

### Task 7: Create A2UI Basic Catalog components

**Files:**
- Create: `web-lit/src/components/agent/a2ui/catalog/basic/a2ui-text.ts`
- Create: `web-lit/src/components/agent/a2ui/catalog/basic/a2ui-row.ts`
- Create: `web-lit/src/components/agent/a2ui/catalog/basic/a2ui-column.ts`
- Create: `web-lit/src/components/agent/a2ui/catalog/basic/a2ui-card.ts`
- Create: `web-lit/src/components/agent/a2ui/catalog/basic/a2ui-button.ts`
- Create: `web-lit/src/components/agent/a2ui/catalog/basic/a2ui-divider.ts`

- [ ] **Step 1: Write a2ui-text.ts**

```typescript
import { LitElement, html, css } from 'lit'
import { customElement, property } from 'lit/decorators.js'

@customElement('a2ui-text')
export class A2uiText extends LitElement {
  @property({ type: String }) text = ''
  @property({ type: String }) variant: 'body' | 'caption' | 'heading' = 'body'

  static styles = css`
    :host { display: block; }
    .body { font-size: 0.875rem; line-height: 1.5; }
    .caption { font-size: 0.75rem; color: var(--text-muted, #888); }
    .heading { font-size: 1rem; font-weight: 600; }
  `

  render() {
    return html`<span class="${this.variant}">${this.text}</span>`
  }
}
```

- [ ] **Step 2: Write a2ui-row.ts**

```typescript
import { LitElement, html, css } from 'lit'
import { customElement } from 'lit/decorators.js'

@customElement('a2ui-row')
export class A2uiRow extends LitElement {
  static styles = css`
    :host { display: block; }
    .row {
      display: flex;
      flex-direction: row;
      gap: 8px;
      align-items: center;
    }
    ::slotted(*) { flex: 0 0 auto; }
  `

  render() {
    return html`<div class="row"><slot></slot></div>`
  }
}
```

- [ ] **Step 3: Write a2ui-column.ts**

```typescript
import { LitElement, html, css } from 'lit'
import { customElement } from 'lit/decorators.js'

@customElement('a2ui-column')
export class A2uiColumn extends LitElement {
  static styles = css`
    :host { display: block; }
    .column {
      display: flex;
      flex-direction: column;
      gap: 8px;
    }
  `

  render() {
    return html`<div class="column"><slot></slot></div>`
  }
}
```

- [ ] **Step 4: Write a2ui-card.ts**

```typescript
import { LitElement, html, css } from 'lit'
import { customElement, property } from 'lit/decorators.js'

@customElement('a2ui-card')
export class A2uiCard extends LitElement {
  @property({ type: String }) title = ''

  static styles = css`
    :host { display: block; }
    .card {
      background: var(--card, #fff);
      border: 1px solid var(--border, #e2e8f0);
      border-radius: var(--radius, 8px);
      padding: 16px;
      box-shadow: 0 1px 3px rgba(0,0,0,0.08);
    }
    .card-title {
      font-size: 0.875rem;
      font-weight: 600;
      margin-bottom: 8px;
      color: var(--text, #1a1a1a);
    }
  `

  render() {
    return html`
      <div class="card">
        ${this.title ? html`<div class="card-title">${this.title}</div>` : ''}
        <slot></slot>
      </div>
    `
  }
}
```

- [ ] **Step 5: Write a2ui-button.ts**

```typescript
import { LitElement, html, css } from 'lit'
import { customElement, property } from 'lit/decorators.js'

@customElement('a2ui-button')
export class A2uiButton extends LitElement {
  @property({ type: String }) label = ''
  @property({ type: String }) variant: 'primary' | 'secondary' | 'danger' = 'primary'
  @property({ type: Boolean }) disabled = false

  static styles = css`
    :host { display: inline-block; }
    button {
      padding: 6px 16px;
      border-radius: var(--radius, 6px);
      font-size: 0.8125rem;
      font-weight: 500;
      cursor: pointer;
      border: 1px solid transparent;
      transition: background 0.15s, border-color 0.15s;
    }
    button.primary {
      background: var(--accent, #6366f1);
      color: #fff;
    }
    button.primary:hover { background: var(--accent-hover, #4f46e5); }
    button.secondary {
      background: transparent;
      color: var(--text, #1a1a1a);
      border-color: var(--border, #e2e8f0);
    }
    button.danger {
      background: var(--danger, #ef4444);
      color: #fff;
    }
    button:disabled {
      opacity: 0.5;
      cursor: not-allowed;
    }
  `

  private _handleClick() {
    this.dispatchEvent(new CustomEvent('a2ui-action', {
      detail: { action: 'click', label: this.label },
      bubbles: true,
      composed: true,
    }))
  }

  render() {
    return html`
      <button
        class="${this.variant}"
        ?disabled="${this.disabled}"
        @click="${this._handleClick}"
      >${this.label}</button>
    `
  }
}
```

- [ ] **Step 6: Write a2ui-divider.ts**

```typescript
import { LitElement, html, css } from 'lit'
import { customElement } from 'lit/decorators.js'

@customElement('a2ui-divider')
export class A2uiDivider extends LitElement {
  static styles = css`
    :host { display: block; }
    hr {
      border: none;
      border-top: 1px solid var(--border, #e2e8f0);
      margin: 8px 0;
    }
  `

  render() {
    return html`<hr />`
  }
}
```

- [ ] **Step 7: Verify TypeScript compiles**

```bash
cd web-lit && npx tsc --noEmit src/components/agent/a2ui/catalog/basic/*.ts 2>&1
```

Expected: no errors.

- [ ] **Step 8: Commit**

```bash
git add web-lit/src/components/agent/a2ui/catalog/basic/
git commit -m "feat(web-lit): add A2UI Basic Catalog components (Text/Row/Column/Card/Button/Divider)"
```

---

### Task 8: Create A2UI IoT Catalog components

**Files:**
- Create: `web-lit/src/components/agent/a2ui/catalog/device-card.ts`
- Create: `web-lit/src/components/agent/a2ui/catalog/device-table.ts`
- Create: `web-lit/src/components/agent/a2ui/catalog/data-chart.ts`
- Create: `web-lit/src/components/agent/a2ui/catalog/control-panel.ts`
- Create: `web-lit/src/components/agent/a2ui/catalog/confirmation-dialog.ts`
- Create: `web-lit/src/components/agent/a2ui/catalog/progress-indicator.ts`
- Create: `web-lit/src/components/agent/a2ui/catalog/real-time-toggle.ts`

- [ ] **Step 1: Write device-card.ts**

```typescript
import { LitElement, html, css } from 'lit'
import { customElement, property } from 'lit/decorators.js'
import type { DeviceProperty } from '../../../../types/agent-types'

@customElement('device-card')
export class DeviceCard extends LitElement {
  @property({ type: String }) deviceId = ''
  @property({ type: String }) name = ''
  @property({ type: String }) status: 'online' | 'offline' | 'warning' | 'error' = 'offline'
  @property({ type: String }) deviceType = 'generic'
  @property({ type: String }) protocol = ''
  @property({ type: String }) lastSeen = ''
  @property({ type: Array }) properties: DeviceProperty[] = []
  @property({ type: Boolean }) showActions = true
  @property({ type: Boolean }) compact = false

  static styles = css`
    :host { display: block; }
    .card {
      background: var(--card, #fff);
      border: 1px solid var(--border, #e2e8f0);
      border-radius: var(--radius, 8px);
      padding: 12px;
    }
    .header {
      display: flex;
      align-items: center;
      gap: 8px;
      margin-bottom: 8px;
    }
    .status-dot {
      width: 8px;
      height: 8px;
      border-radius: 50%;
      flex-shrink: 0;
    }
    .status-dot.online { background: var(--ok, #22c55e); }
    .status-dot.offline { background: var(--text-muted, #94a3b8); }
    .status-dot.warning { background: var(--warn, #f59e0b); }
    .status-dot.error { background: var(--danger, #ef4444); }
    .name { font-weight: 600; font-size: 0.875rem; }
    .meta { font-size: 0.75rem; color: var(--text-muted, #888); }
    .props-table {
      width: 100%;
      font-size: 0.75rem;
      border-collapse: collapse;
    }
    .props-table td {
      padding: 2px 0;
    }
    .props-table td:last-child {
      text-align: right;
      font-family: monospace;
    }
    .actions {
      margin-top: 8px;
      display: flex;
      gap: 6px;
    }
    .actions button {
      font-size: 0.75rem;
      padding: 3px 8px;
      border-radius: 4px;
      border: 1px solid var(--border, #e2e8f0);
      background: transparent;
      cursor: pointer;
      color: var(--text, #1a1a1a);
    }
    .actions button:hover { background: var(--bg-elevated, #f8fafc); }
  `

  private _formatTime(iso: string): string {
    if (!iso) return '-'
    try {
      const d = new Date(iso)
      return d.toLocaleString()
    } catch {
      return iso
    }
  }

  private _handleAction(command: string) {
    this.dispatchEvent(new CustomEvent('a2ui-action', {
      detail: { action: 'command', deviceId: this.deviceId, command },
      bubbles: true, composed: true,
    }))
  }

  render() {
    return html`
      <div class="card">
        <div class="header">
          <span class="status-dot ${this.status}"></span>
          <span class="name">${this.name}</span>
        </div>
        ${!this.compact ? html`
          <div class="meta">${this.deviceType}${this.protocol ? ` · ${this.protocol}` : ''} · ${this._formatTime(this.lastSeen)}</div>
          ${this.properties.length > 0 ? html`
            <table class="props-table">
              ${this.properties.map(p => html`
                <tr>
                  <td>${p.displayName || p.name}</td>
                  <td>${p.currentValue ?? p.value ?? '-'} ${p.unit ?? ''}</td>
                </tr>
              `)}
            </table>
          ` : ''}
          ${this.showActions ? html`
            <div class="actions">
              <button @click="${() => this._handleAction('refresh')}">刷新</button>
              <button @click="${() => this._handleAction('detail')}">详情</button>
            </div>
          ` : ''}
        ` : ''}
      </div>
    `
  }
}
```

- [ ] **Step 2: Write device-table.ts**

```typescript
import { LitElement, html, css } from 'lit'
import { customElement, property } from 'lit/decorators.js'

interface TableColumn {
  key: string
  label: string
}

interface TableRow {
  [key: string]: string | number | boolean
}

@customElement('device-table')
export class DeviceTable extends LitElement {
  @property({ type: Array }) columns: TableColumn[] = []
  @property({ type: Array }) rows: TableRow[] = []
  @property({ type: Number }) page = 1
  @property({ type: Number }) pageSize = 10
  @property({ type: Number }) totalCount = 0

  static styles = css`
    :host { display: block; }
    table {
      width: 100%;
      border-collapse: collapse;
      font-size: 0.8125rem;
    }
    th {
      text-align: left;
      padding: 6px 8px;
      border-bottom: 2px solid var(--border, #e2e8f0);
      font-weight: 600;
      font-size: 0.75rem;
      color: var(--text-muted, #888);
      text-transform: uppercase;
      letter-spacing: 0.05em;
    }
    td {
      padding: 6px 8px;
      border-bottom: 1px solid var(--border, #e2e8f0);
    }
    tr:hover td {
      background: var(--bg-elevated, #f8fafc);
    }
    .pagination {
      display: flex;
      justify-content: space-between;
      align-items: center;
      margin-top: 8px;
      font-size: 0.75rem;
      color: var(--text-muted, #888);
    }
    .pagination button {
      padding: 3px 8px;
      border: 1px solid var(--border, #e2e8f0);
      border-radius: 4px;
      background: transparent;
      cursor: pointer;
      font-size: 0.75rem;
      color: var(--text, #1a1a1a);
    }
    .pagination button:disabled { opacity: 0.4; cursor: not-allowed; }
  `

  private get totalPages(): number {
    return Math.ceil(this.totalCount / this.pageSize) || 1
  }

  private _handlePageChange(newPage: number) {
    this.dispatchEvent(new CustomEvent('a2ui-action', {
      detail: { action: 'pageChange', page: newPage },
      bubbles: true, composed: true,
    }))
  }

  render() {
    return html`
      <table>
        <thead>
          <tr>
            ${this.columns.map(col => html`<th>${col.label}</th>`)}
          </tr>
        </thead>
        <tbody>
          ${this.rows.map(row => html`
            <tr>
              ${this.columns.map(col => html`<td>${row[col.key] ?? '-'}</td>`)}
            </tr>
          `)}
        </tbody>
      </table>
      ${this.totalCount > this.pageSize ? html`
        <div class="pagination">
          <span>共 ${this.totalCount} 条</span>
          <div>
            <button
              ?disabled="${this.page <= 1}"
              @click="${() => this._handlePageChange(this.page - 1)}"
            >上一页</button>
            <span style="margin: 0 8px">${this.page} / ${this.totalPages}</span>
            <button
              ?disabled="${this.page >= this.totalPages}"
              @click="${() => this._handlePageChange(this.page + 1)}"
            >下一页</button>
          </div>
        </div>
      ` : ''}
    `
  }
}
```

- [ ] **Step 3: Write data-chart.ts**

```typescript
import { LitElement, html, css } from 'lit'
import { customElement, property } from 'lit/decorators.js'

interface ChartSeries {
  name: string
  color?: string
  data: number[]
}

@customElement('data-chart')
export class DataChart extends LitElement {
  @property({ type: String }) title = ''
  @property({ type: Array }) series: ChartSeries[] = []
  @property({ type: Array }) labels: string[] = []
  @property({ type: Object }) stats: { min?: number; max?: number; avg?: number } = {}

  static styles = css`
    :host { display: block; }
    .chart-container {
      background: var(--card, #fff);
      border: 1px solid var(--border, #e2e8f0);
      border-radius: var(--radius, 8px);
      padding: 12px;
    }
    .title { font-size: 0.875rem; font-weight: 600; margin-bottom: 8px; }
    svg { width: 100%; height: 200px; }
    .grid-line { stroke: var(--border, #e2e8f0); stroke-width: 0.5; }
    .data-line { fill: none; stroke-width: 2; stroke-linecap: round; stroke-linejoin: round; }
    .data-point { r: 3; }
    .label { font-size: 10px; fill: var(--text-muted, #888); }
    .tooltip {
      position: absolute;
      background: var(--bg-elevated, #1a1a1a);
      color: #fff;
      padding: 4px 8px;
      border-radius: 4px;
      font-size: 0.75rem;
      pointer-events: none;
    }
    .stats {
      display: flex;
      gap: 16px;
      margin-top: 8px;
      font-size: 0.75rem;
      color: var(--text-muted, #888);
    }
    .stats span { font-family: monospace; color: var(--text, #1a1a1a); }
  `

  private _buildPath(data: number[], width: number, height: number, padding: number): string {
    if (data.length < 2) return ''
    const max = Math.max(...data)
    const min = Math.min(...data)
    const range = max - min || 1
    const stepX = (width - padding * 2) / (data.length - 1)

    return data.map((val, i) => {
      const x = padding + i * stepX
      const y = height - padding - ((val - min) / range) * (height - padding * 2)
      return `${i === 0 ? 'M' : 'L'} ${x.toFixed(1)} ${y.toFixed(1)}`
    }).join(' ')
  }

  render() {
    const width = 400
    const height = 200
    const padding = 20
    const colors = ['#6366f1', '#22c55e', '#f59e0b', '#ef4444', '#8b5cf6']

    return html`
      <div class="chart-container">
        ${this.title ? html`<div class="title">${this.title}</div>` : ''}
        <svg viewBox="0 0 ${width} ${height}" preserveAspectRatio="xMidYMid meet">
          <!-- Grid lines -->
          ${[0, 0.25, 0.5, 0.75, 1].map(pct => {
            const y = padding + pct * (height - padding * 2)
            return html`<line class="grid-line" x1="${padding}" y1="${y}" x2="${width - padding}" y2="${y}" />`
          })}
          <!-- Data series -->
          ${this.series.map((s, si) => html`
            <path
              class="data-line"
              d="${this._buildPath(s.data, width, height, padding)}"
              stroke="${s.color || colors[si % colors.length]}"
            />
          `)}
          <!-- X-axis labels -->
          ${this.labels.length > 0 ? this.labels.map((label, i) => {
            const x = padding + i * ((width - padding * 2) / Math.max(this.labels.length - 1, 1))
            return html`<text class="label" x="${x}" y="${height - 4}" text-anchor="middle">${label}</text>`
          }) : ''}
        </svg>
        ${this.stats.min !== undefined ? html`
          <div class="stats">
            <div>Min: <span>${this.stats.min?.toFixed(1)}</span></div>
            <div>Max: <span>${this.stats.max?.toFixed(1)}</span></div>
            <div>Avg: <span>${this.stats.avg?.toFixed(1)}</span></div>
          </div>
        ` : ''}
      </div>
    `
  }
}
```

- [ ] **Step 4: Write control-panel.ts**

```typescript
import { LitElement, html, css } from 'lit'
import { customElement, property, state } from 'lit/decorators.js'

interface ControlAction {
  id: string
  label: string
  command: string
}

@customElement('control-panel')
export class ControlPanel extends LitElement {
  @property({ type: String }) deviceId = ''
  @property({ type: Boolean }) isOn = false
  @property({ type: Number }) sliderValue = 0
  @property({ type: Number }) sliderMin = 0
  @property({ type: Number }) sliderMax = 100
  @property({ type: String }) sliderUnit = ''
  @property({ type: Array }) actions: ControlAction[] = []
  @state() private loading = false

  static styles = css`
    :host { display: block; }
    .panel {
      background: var(--card, #fff);
      border: 1px solid var(--border, #e2e8f0);
      border-radius: var(--radius, 8px);
      padding: 12px;
    }
    .toggle-row {
      display: flex;
      align-items: center;
      justify-content: space-between;
      margin-bottom: 12px;
    }
    .toggle-label { font-size: 0.875rem; font-weight: 500; }
    .toggle {
      width: 40px;
      height: 22px;
      border-radius: 11px;
      border: none;
      cursor: pointer;
      position: relative;
      transition: background 0.2s;
    }
    .toggle.on { background: var(--ok, #22c55e); }
    .toggle.off { background: var(--border, #94a3b8); }
    .toggle::after {
      content: '';
      position: absolute;
      top: 2px;
      width: 18px;
      height: 18px;
      border-radius: 50%;
      background: #fff;
      transition: left 0.2s;
    }
    .toggle.on::after { left: 20px; }
    .toggle.off::after { left: 2px; }
    .slider-row {
      display: flex;
      align-items: center;
      gap: 8px;
      margin-bottom: 12px;
    }
    .slider-row input[type="range"] { flex: 1; }
    .slider-value { font-size: 0.75rem; font-family: monospace; min-width: 40px; text-align: right; }
    .actions { display: flex; gap: 6px; flex-wrap: wrap; }
    .actions button {
      padding: 5px 12px;
      border-radius: 4px;
      border: 1px solid var(--border, #e2e8f0);
      background: transparent;
      cursor: pointer;
      font-size: 0.8125rem;
      color: var(--text, #1a1a1a);
    }
    .actions button:hover { background: var(--bg-elevated, #f8fafc); }
    .actions button:disabled { opacity: 0.5; cursor: not-allowed; }
  `

  private _handleToggle() {
    this.dispatchEvent(new CustomEvent('a2ui-action', {
      detail: { action: 'toggle', deviceId: this.deviceId, value: !this.isOn },
      bubbles: true, composed: true,
    }))
  }

  private _handleSlider(e: Event) {
    const value = Number((e.target as HTMLInputElement).value)
    this.dispatchEvent(new CustomEvent('a2ui-action', {
      detail: { action: 'slider', deviceId: this.deviceId, value },
      bubbles: true, composed: true,
    }))
  }

  private _handleAction(action: ControlAction) {
    this.dispatchEvent(new CustomEvent('a2ui-action', {
      detail: { action: 'command', deviceId: this.deviceId, command: action.command },
      bubbles: true, composed: true,
    }))
  }

  render() {
    return html`
      <div class="panel">
        <div class="toggle-row">
          <span class="toggle-label">电源</span>
          <button
            class="toggle ${this.isOn ? 'on' : 'off'}"
            @click="${this._handleToggle}"
          ></button>
        </div>
        <div class="slider-row">
          <input
            type="range"
            min="${this.sliderMin}"
            max="${this.sliderMax}"
            value="${this.sliderValue}"
            @input="${this._handleSlider}"
          />
          <span class="slider-value">${this.sliderValue}${this.sliderUnit}</span>
        </div>
        ${this.actions.length > 0 ? html`
          <div class="actions">
            ${this.actions.map(a => html`
              <button
                ?disabled="${this.loading}"
                @click="${() => this._handleAction(a)}"
              >${a.label}</button>
            `)}
          </div>
        ` : ''}
      </div>
    `
  }
}
```

- [ ] **Step 5: Write confirmation-dialog.ts**

```typescript
import { LitElement, html, css } from 'lit'
import { customElement, property, state } from 'lit/decorators.js'

@customElement('confirmation-dialog')
export class ConfirmationDialog extends LitElement {
  @property({ type: String }) title = '确认'
  @property({ type: String }) message = ''
  @property({ type: String }) confirmText = '确认'
  @property({ type: String }) cancelText = '取消'
  @property({ type: String }) level: 'normal' | 'warning' | 'destructive' = 'normal'
  @property({ type: Number }) timeout = 0
  @state() private remaining = 0

  private _timer: number | null = null

  static styles = css`
    :host { display: block; }
    .overlay {
      position: fixed;
      inset: 0;
      background: rgba(0,0,0,0.4);
      display: flex;
      align-items: center;
      justify-content: center;
      z-index: 1000;
    }
    .dialog {
      background: var(--card, #fff);
      border-radius: var(--radius, 8px);
      padding: 20px;
      min-width: 300px;
      max-width: 400px;
      box-shadow: 0 8px 32px rgba(0,0,0,0.2);
    }
    .title { font-size: 1rem; font-weight: 600; margin-bottom: 8px; }
    .message { font-size: 0.875rem; color: var(--text-muted, #666); margin-bottom: 16px; }
    .buttons { display: flex; gap: 8px; justify-content: flex-end; }
    button {
      padding: 6px 16px;
      border-radius: 6px;
      border: 1px solid var(--border, #e2e8f0);
      background: transparent;
      cursor: pointer;
      font-size: 0.8125rem;
    }
    button.confirm { color: #fff; border-color: transparent; }
    button.confirm.normal { background: var(--accent, #6366f1); }
    button.confirm.warning { background: var(--warn, #f59e0b); }
    button.confirm.destructive { background: var(--danger, #ef4444); }
  `

  connectedCallback() {
    super.connectedCallback()
    if (this.timeout > 0) {
      this.remaining = this.timeout
      this._timer = window.setInterval(() => {
        this.remaining--
        if (this.remaining <= 0) {
          this._handleCancel()
        }
      }, 1000)
    }
  }

  disconnectedCallback() {
    super.disconnectedCallback()
    if (this._timer) clearInterval(this._timer)
  }

  private _handleConfirm() {
    this.dispatchEvent(new CustomEvent('a2ui-action', {
      detail: { action: 'confirm' },
      bubbles: true, composed: true,
    }))
  }

  private _handleCancel() {
    this.dispatchEvent(new CustomEvent('a2ui-action', {
      detail: { action: 'cancel' },
      bubbles: true, composed: true,
    }))
  }

  render() {
    return html`
      <div class="overlay" @click="${this._handleCancel}">
        <div class="dialog" @click="${(e: Event) => e.stopPropagation()}">
          <div class="title">${this.title}</div>
          <div class="message">${this.message}</div>
          <div class="buttons">
            <button @click="${this._handleCancel}">${this.cancelText}</button>
            <button
              class="confirm ${this.level}"
              @click="${this._handleConfirm}"
            >${this.confirmText}${this.timeout > 0 ? ` (${this.remaining}s)` : ''}</button>
          </div>
        </div>
      </div>
    `
  }
}
```

- [ ] **Step 6: Write progress-indicator.ts**

```typescript
import { LitElement, html, css } from 'lit'
import { customElement, property } from 'lit/decorators.js'

@customElement('progress-indicator')
export class ProgressIndicator extends LitElement {
  @property({ type: Number }) progress = 0
  @property({ type: String }) status = ''
  @property({ type: Number }) current = 0
  @property({ type: Number }) total = 0
  @property({ type: Boolean }) cancellable = false

  static styles = css`
    :host { display: block; }
    .container { padding: 8px 0; }
    .bar-bg {
      height: 6px;
      background: var(--border, #e2e8f0);
      border-radius: 3px;
      overflow: hidden;
    }
    .bar-fill {
      height: 100%;
      background: var(--accent, #6366f1);
      border-radius: 3px;
      transition: width 0.3s ease;
    }
    .info {
      display: flex;
      justify-content: space-between;
      align-items: center;
      margin-top: 6px;
      font-size: 0.75rem;
      color: var(--text-muted, #888);
    }
    .pct { font-family: monospace; }
    button {
      font-size: 0.75rem;
      padding: 2px 8px;
      border-radius: 4px;
      border: 1px solid var(--border, #e2e8f0);
      background: transparent;
      cursor: pointer;
      color: var(--danger, #ef4444);
    }
  `

  private _handleCancel() {
    this.dispatchEvent(new CustomEvent('a2ui-action', {
      detail: { action: 'cancel' },
      bubbles: true, composed: true,
    }))
  }

  render() {
    const pct = Math.min(100, Math.max(0, this.progress))
    return html`
      <div class="container">
        <div class="bar-bg">
          <div class="bar-fill" style="width: ${pct}%"></div>
        </div>
        <div class="info">
          <span>${this.status || '进行中'}</span>
          ${this.total > 0 ? html`<span>${this.current} / ${this.total}</span>` : ''}
          <span class="pct">${pct}%</span>
          ${this.cancellable ? html`<button @click="${this._handleCancel}">取消</button>` : ''}
        </div>
      </div>
    `
  }
}
```

- [ ] **Step 7: Write real-time-toggle.ts**

```typescript
import { LitElement, html, css } from 'lit'
import { customElement, property } from 'lit/decorators.js'

@customElement('real-time-toggle')
export class RealTimeToggle extends LitElement {
  @property({ type: Boolean }) enabled = false
  @property({ type: String }) connectionStatus: 'connected' | 'reconnecting' | 'disconnected' = 'disconnected'

  static styles = css`
    :host { display: inline-block; }
    .container {
      display: flex;
      align-items: center;
      gap: 8px;
    }
    .toggle {
      width: 36px;
      height: 20px;
      border-radius: 10px;
      border: none;
      cursor: pointer;
      position: relative;
      transition: background 0.2s;
    }
    .toggle.on { background: var(--ok, #22c55e); }
    .toggle.off { background: var(--border, #94a3b8); }
    .toggle::after {
      content: '';
      position: absolute;
      top: 2px;
      width: 16px;
      height: 16px;
      border-radius: 50%;
      background: #fff;
      transition: left 0.2s;
    }
    .toggle.on::after { left: 18px; }
    .toggle.off::after { left: 2px; }
    .label { font-size: 0.8125rem; }
    .status-dot {
      width: 6px;
      height: 6px;
      border-radius: 50%;
    }
    .status-dot.connected { background: var(--ok, #22c55e); }
    .status-dot.reconnecting { background: var(--warn, #f59e0b); }
    .status-dot.disconnected { background: var(--text-muted, #94a3b8); }
  `

  private _handleToggle() {
    this.dispatchEvent(new CustomEvent('a2ui-action', {
      detail: { action: 'toggle', value: !this.enabled },
      bubbles: true, composed: true,
    }))
  }

  render() {
    return html`
      <div class="container">
        <button
          class="toggle ${this.enabled ? 'on' : 'off'}"
          @click="${this._handleToggle}"
        ></button>
        <span class="label">实时更新</span>
        <span class="status-dot ${this.connectionStatus}"></span>
      </div>
    `
  }
}
```

- [ ] **Step 8: Verify TypeScript compiles**

```bash
cd web-lit && npx tsc --noEmit src/components/agent/a2ui/catalog/device-card.ts src/components/agent/a2ui/catalog/device-table.ts src/components/agent/a2ui/catalog/data-chart.ts src/components/agent/a2ui/catalog/control-panel.ts src/components/agent/a2ui/catalog/confirmation-dialog.ts src/components/agent/a2ui/catalog/progress-indicator.ts src/components/agent/a2ui/catalog/real-time-toggle.ts 2>&1
```

Expected: no errors.

- [ ] **Step 9: Commit**

```bash
git add web-lit/src/components/agent/a2ui/catalog/device-card.ts web-lit/src/components/agent/a2ui/catalog/device-table.ts web-lit/src/components/agent/a2ui/catalog/data-chart.ts web-lit/src/components/agent/a2ui/catalog/control-panel.ts web-lit/src/components/agent/a2ui/catalog/confirmation-dialog.ts web-lit/src/components/agent/a2ui/catalog/progress-indicator.ts web-lit/src/components/agent/a2ui/catalog/real-time-toggle.ts
git commit -m "feat(web-lit): add A2UI IoT Catalog components (7 components)"
```

---

### Task 9: Create A2UI engine (registry + factory + surface)

**Files:**
- Create: `web-lit/src/components/agent/a2ui/catalog/index.ts`
- Create: `web-lit/src/components/agent/a2ui/a2ui-component.ts`
- Create: `web-lit/src/components/agent/a2ui/a2ui-surface.ts`

- [ ] **Step 1: Write catalog/index.ts**

```typescript
/**
 * A2UI Catalog Registry
 * Maps component type strings to Lit component classes
 */

// Basic Catalog
import './basic/a2ui-text'
import './basic/a2ui-row'
import './basic/a2ui-column'
import './basic/a2ui-card'
import './basic/a2ui-button'
import './basic/a2ui-divider'

// IoT Catalog
import './device-card'
import './device-table'
import './data-chart'
import './control-panel'
import './confirmation-dialog'
import './progress-indicator'
import './real-time-toggle'

const registry = new Map<string, string>()

// Basic Catalog (type → tag name)
registry.set('Text', 'a2ui-text')
registry.set('Row', 'a2ui-row')
registry.set('Column', 'a2ui-column')
registry.set('Card', 'a2ui-card')
registry.set('Button', 'a2ui-button')
registry.set('Divider', 'a2ui-divider')

// IoT Catalog
registry.set('DeviceCard', 'device-card')
registry.set('DeviceTable', 'device-table')
registry.set('DataChart', 'data-chart')
registry.set('ControlPanel', 'control-panel')
registry.set('ConfirmationDialog', 'confirmation-dialog')
registry.set('ProgressIndicator', 'progress-indicator')
registry.set('RealTimeToggle', 'real-time-toggle')

export function getTagName(type: string): string | undefined {
  return registry.get(type)
}

export function getRegisteredTypes(): string[] {
  return Array.from(registry.keys())
}
```

- [ ] **Step 2: Write a2ui-component.ts**

```typescript
/**
 * A2UI Component Factory
 * Dynamically renders a single A2UI component by type
 */

import { LitElement, html, css } from 'lit'
import { customElement, property } from 'lit/decorators.js'
import type { A2uiComponentDescriptor } from '../../../types/agent-types'
import { getTagName } from './catalog/index'

@customElement('a2ui-component')
export class A2uiComponent extends LitElement {
  @property({ type: Object }) descriptor: A2uiComponentDescriptor | null = null

  static styles = css`
    :host { display: block; }
    .unknown {
      padding: 8px;
      background: var(--bg-elevated, #f8fafc);
      border: 1px dashed var(--border, #e2e8f0);
      border-radius: 4px;
      font-size: 0.75rem;
      color: var(--text-muted, #888);
    }
  `

  private _renderComponent() {
    if (!this.descriptor) return null
    const { type, props, children, id } = this.descriptor
    const tagName = getTagName(type)

    if (!tagName) {
      return html`<div class="unknown">[Unknown: ${type}]</div>`
    }

    const el = document.createElement(tagName)
    if (props) {
      for (const [key, value] of Object.entries(props)) {
        (el as any)[key] = value
      }
    }
    el.setAttribute('data-a2ui-id', id)

    // Delegate a2ui-action events upward
    el.addEventListener('a2ui-action', (e: Event) => {
      const ce = e as CustomEvent
      this.dispatchEvent(new CustomEvent('a2ui-action', {
        detail: {
          componentId: id,
          ...ce.detail,
        },
        bubbles: true,
        composed: true,
      }))
    })

    // Render children inside slotted content
    if (children && children.length > 0) {
      // Children will be appended to the element's slot
      // This works for layout components (Row, Column, Card) that have <slot>
      for (const childDesc of children) {
        const childEl = document.createElement('a2ui-component') as A2uiComponent
        childEl.descriptor = childDesc
        el.appendChild(childEl)
      }
    }

    return html`${el}`
  }

  render() {
    return this._renderComponent()
  }
}
```

- [ ] **Step 3: Write a2ui-surface.ts**

```typescript
/**
 * A2UI Surface Manager
 * Manages component lifecycle within a single A2UI surface
 */

import { LitElement, html, css } from 'lit'
import { customElement, property, state } from 'lit/decorators.js'
import type { A2uiComponentDescriptor } from '../../../types/agent-types'

@customElement('a2ui-surface')
export class A2uiSurface extends LitElement {
  @property({ type: String }) surfaceId = ''
  @property({ type: String }) title = ''
  @state() components: A2uiComponentDescriptor[] = []
  @state() dataModel: Record<string, unknown> = {}

  static styles = css`
    :host { display: block; }
    .surface {
      background: var(--card, #fff);
      border: 1px solid var(--border, #e2e8f0);
      border-radius: var(--radius, 8px);
      padding: 12px;
      overflow: hidden;
    }
    .surface-title {
      font-size: 0.8125rem;
      font-weight: 600;
      margin-bottom: 8px;
      color: var(--text-muted, #888);
    }
    .components {
      display: flex;
      flex-direction: column;
      gap: 8px;
    }
  `

  /** Set components externally (from message-group) */
  setComponents(components: A2uiComponentDescriptor[]) {
    this.components = components
  }

  /** Set data model externally */
  setDataModel(data: Record<string, unknown>) {
    this.dataModel = data
  }

  render() {
    return html`
      <div class="surface" data-surface-id="${this.surfaceId}">
        ${this.title ? html`<div class="surface-title">${this.title}</div>` : ''}
        <div class="components">
          ${this.components.map(comp =>
            html`<a2ui-component .descriptor="${comp}"></a2ui-component>`
          )}
        </div>
      </div>
    `
  }
}
```

- [ ] **Step 4: Verify TypeScript compiles**

```bash
cd web-lit && npx tsc --noEmit src/components/agent/a2ui/catalog/index.ts src/components/agent/a2ui/a2ui-component.ts src/components/agent/a2ui/a2ui-surface.ts 2>&1
```

Expected: no errors.

- [ ] **Step 5: Commit**

```bash
git add web-lit/src/components/agent/a2ui/catalog/index.ts web-lit/src/components/agent/a2ui/a2ui-component.ts web-lit/src/components/agent/a2ui/a2ui-surface.ts
git commit -m "feat(web-lit): add A2UI engine (registry, component factory, surface manager)"
```

---

### Task 10: Create message-group and streaming-message

**Files:**
- Create: `web-lit/src/components/agent/message-group.ts`
- Create: `web-lit/src/components/agent/streaming-message.ts`

- [ ] **Step 1: Write streaming-message.ts**

```typescript
/**
 * Streaming message component
 * Shows live-updating content with typing cursor
 */

import { LitElement, html, css } from 'lit'
import { customElement, property } from 'lit/decorators.js'
import { toSanitizedMarkdownHtml } from '../../lib/markdown'

@customElement('streaming-message')
export class StreamingMessage extends LitElement {
  @property({ type: String }) content = ''

  static styles = css`
    :host { display: block; }
    .bubble {
      background: var(--bg-elevated, #f8fafc);
      border-radius: 12px;
      padding: 12px 16px;
      max-width: 72%;
    }
    .cursor {
      display: inline-block;
      width: 2px;
      height: 1em;
      background: var(--accent, #6366f1);
      animation: blink 1s step-end infinite;
      vertical-align: text-bottom;
      margin-left: 1px;
    }
    @keyframes blink {
      50% { opacity: 0; }
    }
    .indicator {
      font-size: 0.75rem;
      color: var(--text-muted, #888);
      margin-top: 4px;
    }
    /* Markdown styling */
    .content :deep(pre) {
      background: var(--bg, #f1f5f9);
      border-radius: 6px;
      padding: 8px 12px;
      overflow-x: auto;
      font-size: 0.8125rem;
    }
    .content :deep(code) {
      font-family: 'SF Mono', Consolas, monospace;
      font-size: 0.8125rem;
    }
    .content :deep(p) { margin: 0 0 0.5em; }
    .content :deep(p:last-child) { margin-bottom: 0; }
  `

  render() {
    const htmlContent = this.content ? toSanitizedMarkdownHtml(this.content) : ''
    return html`
      <div class="bubble">
        <div class="content" .innerHTML="${htmlContent}"></div>
        <span class="cursor"></span>
        <div class="indicator">AI 正在输入...</div>
      </div>
    `
  }
}
```

- [ ] **Step 2: Write message-group.ts**

```typescript
/**
 * Message group component
 * Renders a single chat message (user or assistant) with A2UI surfaces
 */

import { LitElement, html, css } from 'lit'
import { customElement, property } from 'lit/decorators.js'
import type { ChatMessage } from '../../types/agent-types'
import { toSanitizedMarkdownHtml } from '../../lib/markdown'
import '../a2ui/a2ui-surface'
import '../a2ui/a2ui-component'

@customElement('message-group')
export class MessageGroup extends LitElement {
  @property({ type: Object }) message: ChatMessage | null = null

  static styles = css`
    :host { display: block; }
    .group {
      display: flex;
      gap: 8px;
      max-width: 100%;
    }
    .group.user {
      flex-direction: row-reverse;
    }
    .avatar {
      width: 32px;
      height: 32px;
      border-radius: 50%;
      display: flex;
      align-items: center;
      justify-content: center;
      font-size: 0.75rem;
      font-weight: 600;
      flex-shrink: 0;
    }
    .avatar.user {
      background: var(--accent, #6366f1);
      color: #fff;
    }
    .avatar.assistant {
      background: var(--bg-elevated, #e2e8f0);
      color: var(--text, #1a1a1a);
    }
    .content {
      display: flex;
      flex-direction: column;
      gap: 4px;
      max-width: 72%;
    }
    .bubble {
      padding: 10px 14px;
      border-radius: 12px;
      font-size: 0.875rem;
      line-height: 1.6;
    }
    .bubble.user {
      background: var(--accent, #6366f1);
      color: #fff;
    }
    .bubble.assistant {
      background: var(--bg-elevated, #f8fafc);
      color: var(--text, #1a1a1a);
    }
    .bubble :deep(pre) {
      background: rgba(0,0,0,0.1);
      border-radius: 6px;
      padding: 8px 12px;
      overflow-x: auto;
      font-size: 0.8125rem;
      margin: 8px 0;
    }
    .bubble :deep(code) {
      font-family: 'SF Mono', Consolas, monospace;
      font-size: 0.8125rem;
    }
    .bubble :deep(p) { margin: 0 0 0.5em; }
    .bubble :deep(p:last-child) { margin-bottom: 0; }
    .bubble :deep(table) {
      border-collapse: collapse;
      width: 100%;
      margin: 8px 0;
    }
    .bubble :deep(th), .bubble :deep(td) {
      border: 1px solid var(--border, #e2e8f0);
      padding: 4px 8px;
      font-size: 0.8125rem;
    }
    .timestamp {
      font-size: 0.6875rem;
      color: var(--text-muted, #888);
    }
    .group.user .timestamp { text-align: right; }
    .surfaces {
      display: flex;
      flex-direction: column;
      gap: 8px;
      margin-top: 4px;
    }
  `

  private _formatTime(ts: number): string {
    return new Date(ts).toLocaleTimeString([], { hour: '2-digit', minute: '2-digit' })
  }

  render() {
    if (!this.message) return ''
    const msg = this.message
    const isUser = msg.role === 'user'

    return html`
      <div class="group ${msg.role}">
        <div class="avatar ${msg.role}">
          ${isUser ? '我' : 'AI'}
        </div>
        <div class="content">
          <div class="bubble ${msg.role}">
            ${isUser
              ? html`${msg.content}`
              : html`<div .innerHTML="${toSanitizedMarkdownHtml(msg.content)}"></div>`
            }
          </div>
          ${!isUser && msg.surfaces && msg.surfaces.size > 0 ? html`
            <div class="surfaces">
              ${Array.from(msg.surfaces.entries()).map(([surfaceId, state]) => html`
                <a2ui-surface
                  .surfaceId="${surfaceId}"
                  .title="${state.title || ''}"
                  .components="${state.components}"
                  .dataModel="${state.dataModel}"
                ></a2ui-surface>
              `)}
            </div>
          ` : ''}
          <div class="timestamp">${this._formatTime(msg.timestamp)}</div>
        </div>
      </div>
    `
  }
}
```

- [ ] **Step 3: Verify TypeScript compiles**

```bash
cd web-lit && npx tsc --noEmit src/components/agent/message-group.ts src/components/agent/streaming-message.ts 2>&1
```

Expected: no errors.

- [ ] **Step 4: Commit**

```bash
git add web-lit/src/components/agent/message-group.ts web-lit/src/components/agent/streaming-message.ts
git commit -m "feat(web-lit): add message-group and streaming-message components"
```

---

### Task 11: Create chat-thread and chat-input

**Files:**
- Create: `web-lit/src/components/agent/chat-thread.ts`
- Create: `web-lit/src/components/agent/chat-input.ts`

- [ ] **Step 1: Write chat-thread.ts**

```typescript
/**
 * Chat thread - scrollable message list
 */

import { LitElement, html, css } from 'lit'
import { customElement, property } from 'lit/decorators.js'
import { repeat } from 'lit/directives/repeat.js'
import type { ChatMessage } from '../../types/agent-types'
import { scheduleChatScroll, handleChatScroll, type ChatScrollHost } from '../../lib/chat-scroll'
import './message-group'
import './streaming-message'

@customElement('chat-thread')
export class ChatThread extends LitElement implements ChatScrollHost {
  @property({ type: Array }) messages: ChatMessage[] = []
  @property({ type: String }) streamingContent = ''
  @property({ type: Boolean }) isStreaming = false

  chatScrollFrame: number | null = null
  chatScrollTimeout: number | null = null
  chatHasAutoScrolled = false
  chatUserNearBottom = true
  chatNewMessagesBelow = false

  static styles = css`
    :host { display: block; flex: 1; overflow: hidden; }
    .chat-thread {
      height: 100%;
      overflow-y: auto;
      padding: 16px 24px;
      display: flex;
      flex-direction: column;
      gap: 16px;
    }
    @media (max-width: 768px) {
      .chat-thread { padding: 12px; gap: 12px; }
    }
  `

  updated(changed: Map<string, unknown>) {
    if (changed.has('messages') || changed.has('streamingContent')) {
      scheduleChatScroll(this, true)
    }
  }

  private _onScroll(e: Event) {
    handleChatScroll(this, e)
  }

  render() {
    return html`
      <div class="chat-thread" @scroll="${this._onScroll}" role="log" aria-live="polite">
        ${repeat(this.messages, m => m.id, msg =>
          html`<message-group .message="${msg}"></message-group>`
        )}
        ${this.isStreaming && this.streamingContent ? html`
          <streaming-message .content="${this.streamingContent}"></streaming-message>
        ` : ''}
      </div>
    `
  }
}
```

- [ ] **Step 2: Write chat-input.ts**

```typescript
/**
 * Chat input - textarea with send/stop button
 */

import { LitElement, html, css } from 'lit'
import { customElement, property, state } from 'lit/decorators.js'

@customElement('chat-input')
export class ChatInput extends LitElement {
  @property({ type: Boolean }) isStreaming = false
  @state() private value = ''

  static styles = css`
    :host { display: block; flex-shrink: 0; }
    .input-area {
      padding: 12px 24px 16px;
      border-top: 1px solid var(--border, #e2e8f0);
      background: var(--bg, #fff);
    }
    .input-row {
      display: flex;
      gap: 8px;
      align-items: flex-end;
    }
    textarea {
      flex: 1;
      resize: none;
      border: 1px solid var(--border, #e2e8f0);
      border-radius: 12px;
      padding: 10px 14px;
      font-size: 0.875rem;
      font-family: inherit;
      line-height: 1.5;
      min-height: 42px;
      max-height: 150px;
      overflow-y: auto;
      background: var(--bg, #fff);
      color: var(--text, #1a1a1a);
      outline: none;
      transition: border-color 0.15s;
    }
    textarea:focus {
      border-color: var(--accent, #6366f1);
    }
    textarea:disabled {
      opacity: 0.5;
      cursor: not-allowed;
    }
    textarea::placeholder {
      color: var(--text-muted, #94a3b8);
    }
    .send-btn {
      width: 42px;
      height: 42px;
      border-radius: 50%;
      border: none;
      display: flex;
      align-items: center;
      justify-content: center;
      cursor: pointer;
      flex-shrink: 0;
      transition: background 0.15s;
    }
    .send-btn.send {
      background: var(--accent, #6366f1);
      color: #fff;
    }
    .send-btn.send:hover {
      background: var(--accent-hover, #4f46e5);
    }
    .send-btn.send:disabled {
      opacity: 0.4;
      cursor: not-allowed;
    }
    .send-btn.stop {
      background: var(--danger, #ef4444);
      color: #fff;
    }
    .send-btn.stop:hover {
      background: #dc2626;
    }
    .send-btn svg {
      width: 18px;
      height: 18px;
    }
    @media (max-width: 768px) {
      .input-area { padding: 8px 12px 12px; }
    }
  `

  private _handleInput(e: Event) {
    const textarea = e.target as HTMLTextAreaElement
    this.value = textarea.value
    // Auto-resize
    textarea.style.height = 'auto'
    textarea.style.height = Math.min(textarea.scrollHeight, 150) + 'px'
  }

  private _handleKeydown(e: KeyboardEvent) {
    if (e.key === 'Enter' && !e.shiftKey) {
      e.preventDefault()
      if (this.isStreaming) {
        this._handleStop()
      } else {
        this._handleSend()
      }
    }
    if (e.key === 'Escape' && this.isStreaming) {
      this._handleStop()
    }
  }

  private _handleSend() {
    const trimmed = this.value.trim()
    if (!trimmed) return
    this.dispatchEvent(new CustomEvent('message-send', {
      detail: { message: trimmed },
      bubbles: true, composed: true,
    }))
    this.value = ''
    // Reset textarea height
    const textarea = this.shadowRoot?.querySelector('textarea')
    if (textarea) {
      textarea.value = ''
      textarea.style.height = 'auto'
    }
  }

  private _handleStop() {
    this.dispatchEvent(new CustomEvent('message-stop', {
      bubbles: true, composed: true,
    }))
  }

  render() {
    return html`
      <div class="input-area">
        <div class="input-row">
          <textarea
            .value="${this.value}"
            ?disabled="${this.isStreaming}"
            placeholder="询问设备状态、告警、数据..."
            aria-label="输入消息"
            rows="1"
            @input="${this._handleInput}"
            @keydown="${this._handleKeydown}"
          ></textarea>
          <button
            class="send-btn ${this.isStreaming ? 'stop' : 'send'}"
            @click="${this.isStreaming ? this._handleStop : this._handleSend}"
            ?disabled="${!this.isStreaming && !this.value.trim()}"
            aria-label="${this.isStreaming ? '停止' : '发送'}"
          >
            ${this.isStreaming ? html`
              <svg viewBox="0 0 24 24" fill="currentColor"><rect x="6" y="6" width="12" height="12" rx="2"/></svg>
            ` : html`
              <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><path d="M5 12h14M12 5l7 7-7 7"/></svg>
            `}
          </button>
        </div>
      </div>
    `
  }
}
```

- [ ] **Step 3: Verify TypeScript compiles**

```bash
cd web-lit && npx tsc --noEmit src/components/agent/chat-thread.ts src/components/agent/chat-input.ts 2>&1
```

Expected: no errors.

- [ ] **Step 4: Commit**

```bash
git add web-lit/src/components/agent/chat-thread.ts web-lit/src/components/agent/chat-input.ts
git commit -m "feat(web-lit): add chat-thread and chat-input components"
```

---

### Task 12: Create agent page

**Files:**
- Create: `web-lit/src/pages/agent-page.ts`

- [ ] **Step 1: Write agent-page.ts**

```typescript
/**
 * Agent page - AI chat interface with A2UI rendering
 */

import { LitElement, html, css } from 'lit'
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
import { sendAgentMessage, sendAgentAction } from '../services/agent'
import type { A2uiMessage } from '../types/agent-types'
import '../components/agent/chat-thread'
import '../components/agent/chat-input'

@customElement('agent-page')
export class AgentPage extends LitElement {
  @state() private messages = $chatMessages.get()
  @state() private streamingContent = $streamingContent.get()
  @state() private isStreaming = $isStreaming.get()
  private _abortController: AbortController | null = null
  private _unsubs: (() => void)[] = []

  static styles = css`
    :host {
      display: flex;
      flex-direction: column;
      height: 100%;
    }
    .header {
      padding: 12px 24px;
      border-bottom: 1px solid var(--border, #e2e8f0);
      font-size: 0.9375rem;
      font-weight: 600;
      display: flex;
      align-items: center;
      gap: 8px;
      flex-shrink: 0;
    }
    .header svg { width: 20px; height: 20px; color: var(--accent, #6366f1); }
    .main {
      flex: 1;
      overflow: hidden;
      display: flex;
      flex-direction: column;
    }
    /* Welcome state */
    .welcome {
      flex: 1;
      display: flex;
      flex-direction: column;
      align-items: center;
      justify-content: center;
      padding: 40px 24px;
      text-align: center;
    }
    .welcome-icon {
      width: 64px;
      height: 64px;
      border-radius: 16px;
      background: linear-gradient(135deg, var(--accent, #6366f1), #8b5cf6);
      display: flex;
      align-items: center;
      justify-content: center;
      margin-bottom: 16px;
    }
    .welcome-icon svg { width: 32px; height: 32px; color: #fff; }
    .welcome h2 {
      font-size: 1.25rem;
      font-weight: 600;
      margin: 0 0 8px;
      color: var(--text, #1a1a1a);
    }
    .welcome p {
      font-size: 0.875rem;
      color: var(--text-muted, #888);
      margin: 0 0 24px;
    }
    .suggestions {
      display: flex;
      flex-wrap: wrap;
      gap: 8px;
      justify-content: center;
      max-width: 500px;
    }
    .suggestion {
      padding: 8px 16px;
      border: 1px solid var(--border, #e2e8f0);
      border-radius: 20px;
      font-size: 0.8125rem;
      background: transparent;
      cursor: pointer;
      color: var(--text, #1a1a1a);
      transition: border-color 0.15s, background 0.15s;
    }
    .suggestion:hover {
      border-color: var(--accent, #6366f1);
      background: var(--bg-elevated, #f8fafc);
    }
    @media (max-width: 768px) {
      .suggestions { max-width: 100%; }
    }
  `

  connectedCallback() {
    super.connectedCallback()
    loadMessagesFromStorage()
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

  private async _handleSend(e: CustomEvent) {
    const message = e.detail.message
    addMessage({ role: 'user', content: message })
    addMessage({ role: 'assistant', content: '', isStreaming: true })
    $isStreaming.set(true)

    this._abortController = new AbortController()

    const onDelta = (content: string) => appendStreamDelta(content)
    const onA2ui = (msg: A2uiMessage) => {
      // Extract surfaceId from the message payload
      const surfaceId = (msg.payload as any)?.surfaceId || 'default'
      addA2uiToLastMessage(surfaceId, msg)
    }
    const onFinal = (content: string) => {
      // Final content replaces streamed content
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
    } catch (err: any) {
      if (err.name === 'AbortError') {
        // User stopped the stream — keep partial content
        finalizeStream()
        return
      }
      // Show error in last message
      const messages = $chatMessages.get()
      const last = messages[messages.length - 1]
      if (last && last.isStreaming) {
        last.content = `连接失败：${err.message || '请重试'}`
        last.isStreaming = false
        $chatMessages.set([...messages])
      }
      $streamingContent.set('')
      $isStreaming.set(false)
    }
  }

  private _handleStop() {
    this._abortController?.abort()
  }

  private _handleSuggestion(text: string) {
    this.dispatchEvent(new CustomEvent('message-send', {
      detail: { message: text },
      bubbles: true, composed: true,
    }))
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
```

- [ ] **Step 2: Verify TypeScript compiles**

```bash
cd web-lit && npx tsc --noEmit src/pages/agent-page.ts 2>&1
```

Expected: no errors.

- [ ] **Step 3: Commit**

```bash
git add web-lit/src/pages/agent-page.ts
git commit -m "feat(web-lit): add agent page with welcome state and SSE integration"
```

---

### Task 13: Register route and sidebar navigation

**Files:**
- Modify: `web-lit/src/app.ts`
- Modify: `web-lit/src/components/app-sidebar.ts`

- [ ] **Step 1: Add import to app.ts**

Add after the existing page imports (line ~29):

```typescript
import './pages/agent-page'
```

- [ ] **Step 2: Add route case to renderPage() in app.ts**

In the `renderPage()` switch statement, add before the `default:` case (line ~159):

```typescript
      case 'agent':
        return html`<agent-page></agent-page>`
```

- [ ] **Step 3: Add AI assistant nav item to app-sidebar.ts**

Add a new section between the "运维管理" section and the "应用中心" section (after line ~54, before line ~55):

```typescript
    {
      items: [
        { id: 'agent', icon: 'agent', label: 'AI 助手', route: 'agent' },
      ],
    },
```

- [ ] **Step 4: Add agent icon to renderIcon() in app-sidebar.ts**

Add to the icons map (around line ~111):

```typescript
      'agent': html`<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.5"><path d="M9.813 15.904L9 18.75l-.813-2.846a4.5 4.5 0 00-3.09-3.09L2.25 12l2.846-.813a4.5 4.5 0 003.09-3.09L9 5.25l.813 2.846a4.5 4.5 0 003.09 3.09L15.75 12l-2.846.813a4.5 4.5 0 00-3.09 3.09zM18.259 8.715L18 9.75l-.259-1.035a3.375 3.375 0 00-2.455-2.456L14.25 6l1.036-.259a3.375 3.375 0 002.455-2.456L18 2.25l.259 1.035a3.375 3.375 0 002.455 2.456L21.75 6l-1.036.259a3.375 3.375 0 00-2.455 2.456z"/></svg>`,
```

- [ ] **Step 5: Verify build**

```bash
cd web-lit && npx vite build 2>&1 | tail -10
```

Expected: build succeeds.

- [ ] **Step 6: Commit**

```bash
git add web-lit/src/app.ts web-lit/src/components/app-sidebar.ts
git commit -m "feat(web-lit): register agent route and add sidebar navigation"
```

---

### Task 14: Build verification and CSS polish

**Files:**
- Modify: `web-lit/src/styles/base.css` (if needed for A2UI tokens)
- Modify: `web-lit/src/styles/layout.css` (if needed)

- [ ] **Step 1: Run full build**

```bash
cd web-lit && npx vite build 2>&1
```

Expected: build succeeds with no errors.

- [ ] **Step 2: Run all tests**

```bash
cd web-lit && npx vitest run 2>&1
```

Expected: all tests pass.

- [ ] **Step 3: Verify all new custom elements are registered**

```bash
cd web-lit && grep -r "@customElement" src/components/agent/ src/pages/agent-page.ts | wc -l
```

Expected: 17 custom elements (chat-thread, chat-input, message-group, streaming-message, a2ui-surface, a2ui-component, device-card, device-table, data-chart, control-panel, confirmation-dialog, progress-indicator, real-time-toggle, a2ui-text, a2ui-row, a2ui-column, a2ui-card, a2ui-button, a2ui-divider = 19).

- [ ] **Step 4: Check CSS variables used in A2UI components exist in base.css**

```bash
cd web-lit && grep -E "(--bg-elevated|--card|--text-muted|--accent-hover|--ok|--warn|--danger|--radius|--border|--text|--accent|--bg)" src/styles/base.css | head -20
```

Verify all CSS variables referenced by the A2UI components exist. Add any missing ones to both `:root` and `:root[data-theme-mode="light"]` sections.

- [ ] **Step 5: Final commit**

```bash
git add -A
git commit -m "chore(web-lit): build verification and CSS variable audit for agent page"
```

---

## Spec Coverage Check

| Spec Section | Task |
|---|---|
| Route registration | Task 13 |
| Sidebar navigation | Task 13 |
| sessionStorage persistence | Task 3 (store), Task 12 (page) |
| SSE communication layer | Task 6 |
| Agent store (nanostore) | Task 3 |
| Markdown rendering | Task 4 |
| Auto-scroll | Task 5 |
| Type definitions | Task 2 |
| A2UI Basic Catalog (6 components) | Task 7 |
| A2UI IoT Catalog (7 components) | Task 8 |
| Catalog Registry | Task 9 |
| Component Factory | Task 9 |
| Surface Manager | Task 9 |
| Message group rendering | Task 10 |
| Streaming message | Task 10 |
| Chat thread | Task 11 |
| Chat input | Task 11 |
| Agent page + welcome state | Task 12 |
| Error handling (SSE) | Task 6, Task 12 |
| A2UI event feedback | Task 8 (components), Task 6 (sendAgentAction) |
| Dependencies (marked, dompurify) | Task 1 |
