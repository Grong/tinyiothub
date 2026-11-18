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
import { marked } from 'marked';
import DOMPurify from 'dompurify';
import hljs from 'highlight.js/lib/core';
import bash from 'highlight.js/lib/languages/bash';
import css from 'highlight.js/lib/languages/css';
import javascript from 'highlight.js/lib/languages/javascript';
import json from 'highlight.js/lib/languages/json';
import python from 'highlight.js/lib/languages/python';
import rust from 'highlight.js/lib/languages/rust';
import typescript from 'highlight.js/lib/languages/typescript';
import xml from 'highlight.js/lib/languages/xml';
import yaml from 'highlight.js/lib/languages/yaml';

for (const [name, language] of Object.entries({
  bash,
  css,
  javascript,
  json,
  python,
  rust,
  typescript,
  xml,
  yaml,
})) {
  hljs.registerLanguage(name, language);
}

/** Configure marked once at module load. */
marked.setOptions({ async: false, gfm: true, breaks: true });
marked.use({
  renderer: {
    code({ text, lang }) {
      const language = lang?.trim().split(/\s+/, 1)[0].toLowerCase() ?? '';
      if (!language || !hljs.getLanguage(language)) return false;
      try {
        return `<pre><code class="hljs language-${language}">${hljs.highlight(text, { language }).value}</code></pre>`;
      } catch {
        return false;
      }
    },
  },
});

/**
 * F8 外链收口（内部管理工具不加载第三方资源）：
 * - <a> 一律 rel="noopener noreferrer" target="_blank"（防 tabnabbing + 新窗打开）
 * - 外链 <img>（http/https src）剥离；data:/相对路径保留
 */
DOMPurify.addHook('afterSanitizeAttributes', (node) => {
  if (node.tagName === 'A') {
    node.setAttribute('rel', 'noopener noreferrer');
    node.setAttribute('target', '_blank');
  }
  if (node.tagName === 'IMG') {
    const src = node.getAttribute('src') ?? '';
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
