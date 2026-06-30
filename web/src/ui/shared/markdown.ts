/**
 * Shared Markdown renderer — DOMPurify + marked.
 *
 * Use this single helper everywhere instead of duplicating the md() function
 * across views and renderers.
 */
import { marked } from "marked";
import DOMPurify from "dompurify";

/** Configure marked once at module load. */
marked.setOptions({ async: false, gfm: true });

/**
 * Parse Markdown text to sanitized HTML.
 * Safe for use with lit's `unsafeHTML` directive.
 */
export function md(text: string): string {
  try {
    return DOMPurify.sanitize(marked.parse(text) as string);
  } catch {
    return DOMPurify.sanitize(text);
  }
}
