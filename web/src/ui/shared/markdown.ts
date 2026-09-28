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
 * F8 外链收口（内部管理工具不加载第三方资源）：
 * - <a> 一律 rel="noopener noreferrer" target="_blank"（防 tabnabbing + 新窗打开）
 * - 外链 <img>（http/https src）剥离；data:/相对路径保留
 */
DOMPurify.addHook("afterSanitizeAttributes", (node) => {
  if (node.tagName === "A") {
    node.setAttribute("rel", "noopener noreferrer");
    node.setAttribute("target", "_blank");
  }
  if (node.tagName === "IMG") {
    const src = node.getAttribute("src") ?? "";
    if (/^https?:\/\//i.test(src)) {
      node.remove();
    }
  }
});

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
