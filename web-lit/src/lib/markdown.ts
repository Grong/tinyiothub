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
