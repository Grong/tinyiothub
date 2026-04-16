import { html, nothing, type TemplateResult } from "lit";
import { unsafeHTML } from "lit/directives/unsafe-html.js";
import { marked } from "marked";
import DOMPurify from "dompurify";
import type { ChatMessage } from "../controllers/chat.js";
import type { A2uiRendererEngine } from "./a2ui/a2ui-renderer.js";

export type MessageGroup = {
  role: string;
  messages: ChatMessage[];
  firstTimestamp: number;
};

// Configure marked
marked.setOptions({ async: false, gfm: true });

function toMarkdownHtml(text: string): string {
  try {
    const raw = marked.parse(text) as string;
    return DOMPurify.sanitize(raw);
  } catch {
    return DOMPurify.sanitize(text);
  }
}

// ============================================================================
// Grouping
// ============================================================================

export function groupMessages(messages: ChatMessage[]): MessageGroup[] {
  const groups: MessageGroup[] = [];
  let currentGroup: MessageGroup | null = null;

  for (const msg of messages) {
    const role = normalizeRole(msg.role);
    if (currentGroup && currentGroup.role === role) {
      currentGroup.messages.push(msg);
    } else {
      currentGroup = { role, messages: [msg], firstTimestamp: msg.timestamp || Date.now() };
      groups.push(currentGroup);
    }
  }

  return groups;
}

function normalizeRole(role: string): string {
  const r = role.toLowerCase();
  if (r === "tool" || r === "toolresult" || r === "tool_result") return "tool";
  if (r === "user") return "user";
  return "assistant";
}

// ============================================================================
// Rendering
// ============================================================================

export function renderMessageGroup(
  group: MessageGroup,
  a2uiRenderer?: A2uiRendererEngine,
): TemplateResult {
  const isUser = group.role === "user";
  const isAssistant = group.role === "assistant";
  const isTool = group.role === "tool";

  const avatarIcon = isUser ? "U" : isAssistant ? "A" : "T";
  const avatarClass = isUser
    ? "chat-avatar--user"
    : isAssistant
      ? "chat-avatar--assistant"
      : "chat-avatar--tool";

  // For tool groups: merge and deduplicate cards from all messages in group
  if (isTool) {
    const allCards = extractToolCardsFromGroup(group.messages);
    return html`
      <div class="chat-group ${group.role}">
        <div class="chat-avatar ${avatarClass}">${avatarIcon}</div>
        <div class="chat-group-messages">
          ${allCards.length > 0
            ? html`<div class="chat-tool-flow">${allCards.map((tc) => renderToolCallChip(tc.name, tc.args, tc.result))}</div>`
            : group.messages.map((msg) => renderToolCard(msg))}
        </div>
      </div>
    `;
  }

  return html`
    <div class="chat-group ${group.role}">
      <div class="chat-avatar ${avatarClass}">${avatarIcon}</div>
      <div class="chat-group-messages">
        ${group.messages.map((msg) => renderAssistantMessage(msg, a2uiRenderer))}
      </div>
    </div>
  `;
}

/// Render streaming: text segments + tool call cards interleaved
export function renderStreamingGroup(
  segments: Array<{ text: string; ts: number }>,
  toolOrder: string[],
  toolById: Map<string, { toolName: string; toolArgs: string; toolResult?: string }>,
  currentStream: string,
  _a2uiRenderer?: A2uiRendererEngine,
): TemplateResult {
  const allSegments: Array<TemplateResult | null> = [];

  for (const seg of segments) {
    if (seg.text.trim()) {
      allSegments.push(html`
        <div class="chat-text streaming-text">${unsafeHTML(toMarkdownHtml(seg.text))}</div>
      `);
    }
  }

  for (const id of toolOrder) {
    const tc = toolById.get(id);
    if (tc) {
      allSegments.push(renderToolCallChip(tc.toolName, tc.toolArgs, tc.toolResult));
    }
  }

  if (currentStream.trim()) {
    allSegments.push(html`
      <div class="chat-text streaming-text">${unsafeHTML(toMarkdownHtml(currentStream))}</div>
    `);
  }

  return html`
    <div class="chat-group assistant">
      <div class="chat-avatar chat-avatar--assistant">A</div>
      <div class="chat-group-messages">
        <div class="chat-bubble chat-bubble--assistant">
          ${allSegments.length > 0
            ? allSegments
            : html`<div class="chat-streaming-indicator" aria-hidden="true">
                <span></span><span></span><span></span>
              </div>`}
        </div>
      </div>
    </div>
  `;
}

export function renderReadingIndicatorGroup(): TemplateResult {
  return html`
    <div class="chat-group assistant">
      <div class="chat-avatar chat-avatar--assistant">A</div>
      <div class="chat-group-messages">
        <div class="chat-bubble chat-reading-indicator" aria-hidden="true">
          <span class="chat-reading-indicator__dots">
            <span></span><span></span><span></span>
          </span>
        </div>
      </div>
    </div>
  `;
}

// ============================================================================
// Assistant Message
// ============================================================================

function renderAssistantMessage(msg: ChatMessage, a2uiRenderer?: A2uiRendererEngine): TemplateResult {
  const surfaceContent =
    a2uiRenderer && (msg as any).a2uiSurfaceId
      ? a2uiRenderer.renderSurface((msg as any).a2uiSurfaceId)
      : nothing;

  // Extract thinking (<think> tags in text)
  const thinking = extractThinking(msg);
  // Extract tool cards
  const toolCards = extractToolCards(msg);

  // Extract text content - each block is handled individually
  // If a text block has inline thinking tags, strip only the tags but keep rest
  // If a text block has no thinking tags, keep it as-is
  const textParts: string[] = [];
  for (const block of msg.content) {
    if (block.type === "text" && block.text) {
      if (block.text.includes("<think>")) {
        // Strip <think> tags, keep the rest
        const clean = stripThinkingTags(block.text);
        if (clean) textParts.push(clean);
      } else {
        textParts.push(block.text);
      }
    }
  }
  const text = textParts.join("");

  // Has inline thinking tags (<think> in text blocks)
  const hasInlineThinking = msg.content.some(
    (b) => b.type === "text" && b.text && b.text.includes("<think>")
  );

  return html`
    <div class="chat-bubble chat-bubble--assistant">
      ${thinking && hasInlineThinking
        ? html`<div class="chat-thinking">
            ${unsafeHTML(toMarkdownHtml(thinking))}
          </div>`
        : nothing}
      ${text
        ? html`<div class="chat-text">${unsafeHTML(toMarkdownHtml(text))}</div>`
        : nothing}
      ${thinking && !hasInlineThinking
        ? html`<div class="chat-thinking">
            <div class="chat-thinking-label">思考中...</div>
            ${unsafeHTML(toMarkdownHtml(thinking))}
          </div>`
        : nothing}
      ${surfaceContent}
      ${toolCards.length > 0
        ? html`<div class="chat-tool-cards">
            ${toolCards.map((tc) => renderToolCallChip(tc.name, tc.args, tc.result))}
          </div>`
        : nothing}
      ${msg.timestamp
        ? html`<div class="chat-timestamp">${formatTime(msg.timestamp)}</div>`
        : nothing}
    </div>
  `;
}

// ============================================================================
// Tool Cards — Compact Flow Style
// ============================================================================

type ToolCard = { name: string; args: string; result?: string };

// Tool type icons (SVG paths) and categories
const TOOL_ICONS: Record<string, string> = {
  // Device operations
  get_device: '<svg viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round"><circle cx="8" cy="8" r="3"/><path d="M8 1v2M8 13v2M1 8h2M13 8h2M3.05 3.05l1.41 1.41M11.54 11.54l1.41 1.41M3.05 12.95l1.41-1.41M11.54 4.46l1.41-1.41"/></svg>',
  list_devices: '<svg viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round"><rect x="2" y="2" width="5" height="5" rx="1"/><rect x="9" y="2" width="5" height="5" rx="1"/><rect x="2" y="9" width="5" height="5" rx="1"/><rect x="9" y="9" width="5" height="5" rx="1"/></svg>',
  get_device_properties: '<svg viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round"><path d="M3 3h4M3 7h8M3 11h6M3 15h4"/></svg>',
  set_device_property: '<svg viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round"><path d="M8 3v10M3 8h10"/></svg>',
  execute_command: '<svg viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round"><path d="M6 4l6 4-6 4V4z"/></svg>',
  get_device_history: '<svg viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round"><path d="M2 12l3-3 2 2 4-5 3 3"/><rect x="2" y="2" width="12" height="12" rx="2"/></svg>',
  // Alarm operations
  list_alarms: '<svg viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round"><path d="M8 2a5 5 0 00-5 5v3l-1.5 2h13L13 10v-3a5 5 0 00-5-5zM5 13a1 1 0 106 0M9 13a1 1 0 106 0"/></svg>',
  acknowledge_alarm: '<svg viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round"><path d="M3 8l3 3 7-7"/></svg>',
  resolve_alarm: '<svg viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round"><circle cx="8" cy="8" r="6"/><path d="M5 8l2 2 4-4"/></svg>',
  // Data/Query
  query_data: '<svg viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round"><circle cx="7" cy="7" r="4.5"/><path d="M10.5 10.5l3 3"/></svg>',
  aggregate_data: '<svg viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round"><path d="M2 14V8M6 14V5M10 14V9M14 14V2"/></svg>',
  // System
  system_info: '<svg viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round"><circle cx="8" cy="8" r="6"/><path d="M8 5v3l2 2"/></svg>',
  // Canvas (A2UI) — skip rendering
  canvas: "",
  // Default
};

const TOOL_CATEGORY_LABELS: Record<string, string> = {
  device: "设备",
  alarm: "告警",
  data: "数据",
  system: "系统",
  default: "工具",
};

function getToolCategory(name: string): string {
  const n = name.toLowerCase();
  if (n.includes("device")) return "device";
  if (n.includes("alarm")) return "alarm";
  if (n.includes("query") || n.includes("data") || n.includes("aggregate")) return "data";
  if (n.includes("system")) return "system";
  return "default";
}

function getToolIcon(name: string): string {
  return TOOL_ICONS[name] || "⚙";
}

function extractToolCards(msg: ChatMessage): ToolCard[] {
  // Sequential pairing: toolcall[0] ↔ toolresult[0], toolcall[1] ↔ toolresult[1], ...
  const calls: Array<{ name: string; args: string }> = [];
  const results: Array<string> = [];
  for (const block of msg.content) {
    if (block.type === "toolcall" && block.name) {
      if (block.name === "canvas") continue;
      calls.push({
        name: block.name,
        args: typeof block.args === "string" ? block.args : JSON.stringify(block.args || {}, null, 2),
      });
    }
    if (block.type === "toolresult" && block.result) {
      results.push(typeof block.result === "string" ? block.result : JSON.stringify(block.result, null, 2));
    }
  }
  if (calls.length === 0 && msg.toolName && msg.toolName !== "canvas") {
    const text = msg.content.find((c) => c.type === "text" && c.text)?.text;
    return [{ name: msg.toolName, args: "{}", result: text }];
  }
  return calls.map((call, i) => ({
    name: call.name,
    args: call.args,
    result: results[i] ?? undefined,
  }));
}

// Track expanded chips per unique key
const expandedChips = new Map<string, boolean>();
let _chipVersion = 0;
let _onChipToggle: (() => void) | null = null;

export function setChipToggleCallback(cb: () => void): void {
  _onChipToggle = cb;
}

export function toggleToolChip(name: string, args: string): void {
  const key = `${name}::${args}`;
  expandedChips.set(key, !expandedChips.get(key));
  _chipVersion++;
  _onChipToggle?.();
}

function renderToolCard(msg: ChatMessage): TemplateResult {
  const cards = extractToolCards(msg);
  if (cards.length === 0) return html``;
  return html`<div class="chat-tool-flow">${cards.map((tc) => renderToolCallChip(tc.name, tc.args, tc.result))}</div>`;
}

/// Extract and deduplicate tool cards from a group of messages
function extractToolCardsFromGroup(messages: ChatMessage[]): ToolCard[] {
  // Collect all tool calls and results in order across all messages
  const calls: Array<{ name: string; args: string }> = [];
  const results: Array<string> = [];

  for (const msg of messages) {
    for (const block of msg.content) {
      if (block.type === "toolcall" && block.name) {
        if (block.name === "canvas") continue;
        calls.push({
          name: block.name,
          args: typeof block.args === "string" ? block.args : JSON.stringify(block.args || {}, null, 2),
        });
      }
      if (block.type === "toolresult" && block.result) {
        results.push(typeof block.result === "string" ? block.result : JSON.stringify(block.result, null, 2));
      }
    }
    // Fallback: toolName on message
    if (msg.toolName && msg.toolName !== "canvas" && calls.length === 0 && results.length === 0) {
      const text = msg.content.find((c) => c.type === "text" && c.text)?.text;
      return [{ name: msg.toolName, args: "{}", result: text }];
    }
  }

  // Pair calls with results by order (sequential pairing)
  return calls.map((call, i) => ({
    name: call.name,
    args: call.args,
    result: results[i] ?? undefined,
  }));
}

/// Render a single tool call as a compact inline chip (Lit-controlled expand state)
function renderToolCallChip(name: string, args: string, result?: string): TemplateResult {
  if (name === "canvas") return html``;

  const icon = getToolIcon(name);
  const hasResult = Boolean(result);
  const hasArgs = tryParseArgs(args).length > 0;
  const parsedArgs = tryParseArgs(args);
  const argsPreview = buildArgsPreview(parsedArgs);

  // Use args JSON as stable key (same tool+args = same key)
  const chipKey = `${name}::${args}`;
  const isExpanded = expandedChips.get(chipKey) ?? false;

  return html`
    <div class="chat-tool-chip ${isExpanded ? 'expanded' : ''}">
      <div
        class="chat-tool-chip__summary"
        role="button"
        tabindex="0"
        @click=${() => {
          toggleToolChip(name, args);
        }}
        @keydown=${(e: KeyboardEvent) => {
          if (e.key === "Enter" || e.key === " ") {
            e.preventDefault();
            toggleToolChip(name, args);
          }
        }}
      >
        <span class="chat-tool-chip__icon">${unsafeHTML(icon)}</span>
        <span class="chat-tool-chip__name">${name}</span>
        ${hasArgs ? html`<span class="chat-tool-chip__preview">${argsPreview}</span>` : nothing}
        <span class="chat-tool-chip__status ${hasResult ? 'ok' : 'pending'}">
          ${hasResult
            ? html`<svg viewBox="0 0 16 16" fill="currentColor" width="10" height="10"><path d="M13.78 4.22a.75.75 0 010 1.06l-7.25 7.25a.75.75 0 01-1.06 0L2.22 9.28a.75.75 0 011.06-1.06L6 10.94l6.72-6.72a.75.75 0 011.06 0z"/></svg>`
            : html`<svg class="spin" viewBox="0 0 16 16" fill="currentColor" width="10" height="10"><path d="M8 3a5 5 0 11-5 5h1.5A3.5 3.5 0 008 5.5 3.5 3.5 0 0111.5 5v.5a.5.5 0 01-1 0V5A5.5 5.5 0 008 0a5 5 0 000 3z"/></svg>`}
        </span>
      </div>
      ${isExpanded ? html`
        <div class="chat-tool-chip__body">
          ${hasArgs ? html`
            <div class="chat-tool-chip__section">
              <span class="chat-tool-chip__section-label">参数</span>
              <pre class="chat-tool-chip__args">${args}</pre>
            </div>
          ` : nothing}
          ${result ? html`
            <div class="chat-tool-chip__section">
              <span class="chat-tool-chip__section-label">结果</span>
              <div class="chat-tool-chip__result">${unsafeHTML(toMarkdownHtml(truncateResult(result)))}</div>
            </div>
          ` : html`
            <div class="chat-tool-chip__section chat-tool-chip__section--pending">
              <span>等待执行结果...</span>
            </div>
          `}
        </div>
      ` : nothing}
    </div>
  `;
}

function tryParseArgs(args: string): Record<string, unknown> {
  try {
    return JSON.parse(args);
  } catch {
    return {};
  }
}

function buildArgsPreview(parsed: Record<string, unknown>): string {
  const keys = Object.keys(parsed);
  if (keys.length === 0) return "";
  const preview = keys.slice(0, 2).map(k => {
    const v = parsed[k];
    if (typeof v === "string") return v.length > 15 ? v.slice(0, 15) + "…" : v;
    if (typeof v === "number") return String(v);
    return "";
  }).filter(Boolean).join(" · ");
  return preview || "";
}

function truncateResult(result: string, maxLen = 400): string {
  if (result.length <= maxLen) return result;
  const isJson = result.trimStart().startsWith("{") || result.trimStart().startsWith("[");
  if (isJson) {
    return result.slice(0, maxLen) + "\n…(结果已截断，点击展开查看全部)";
  }
  return result.slice(0, maxLen) + "…(结果已截断)";
}

// ============================================================================
// Thinking extraction
// ============================================================================

function extractThinking(msg: ChatMessage): string | null {
  for (const block of msg.content) {
    if (block.type === "thinking" && typeof block.thinking === "string") {
      return block.thinking;
    }
  }
  // Also extract from text blocks with <think> tags (only the thinking part)
  for (const block of msg.content) {
    if (block.type === "text" && block.text) {
      const matches = [...block.text.matchAll(/<think(?:ing)?\s*>([\s\S]*?)<\/think(?:ing)?\s*>/gi)];
      if (matches.length > 0) {
        return matches.map((m) => m[1] || "").filter(Boolean).join("\n");
      }
    }
  }
  return null;
}

/// Strip <think> tags from text, returning the actual content
function stripThinkingTags(text: string): string {
  return text.replace(/<think(?:ing)?\s*>[\s\S]*?<\/think(?:ing)?\s*>/gi, "").trim();
}

// ============================================================================
// Utilities
// ============================================================================

function formatTime(timestamp: number): string {
  return new Date(timestamp).toLocaleTimeString([], { hour: "numeric", minute: "2-digit" });
}
