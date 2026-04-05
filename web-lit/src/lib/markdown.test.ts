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
