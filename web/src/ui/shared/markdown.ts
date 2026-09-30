/**
 * Shared Markdown renderer — DOMPurify + marked.
 *
 * Use this single helper everywhere instead of duplicating the md() function
 * across views and renderers.
 *
 * ⚠ 全局副作用：本模块注册的 F8 afterSanitizeAttributes 钩子挂在 DOMPurify
 * 单例上——一旦本模块被 import，进程内所有 DOMPurify.sanitize 调用（含不
 * 经本 helper 的调用方）都套用同一策略（a 加 rel/target、外链 img 剥离）。
 * 这是有意的单源消毒姿态（外部声音 #3 已记录：heartbeat 系视图经此继承
 * F8 语义，渲染行为与迁移前有差异）；需要不同策略的消费方不能共享本模块。
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
