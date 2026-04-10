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

  return html`
    <div class="chat-group ${group.role}">
      <div class="chat-avatar ${avatarClass}">${avatarIcon}</div>
      <div class="chat-group-messages">
        ${isTool
          ? group.messages.map((msg) => renderToolCard(msg))
          : group.messages.map((msg) => renderAssistantMessage(msg, a2uiRenderer))}
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
      allSegments.push(renderToolCallCard(tc.toolName, tc.toolArgs, tc.toolResult));
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
            ${toolCards.map((tc) => renderToolCallCard(tc.name, tc.args, tc.result))}
          </div>`
        : nothing}
      ${msg.timestamp
        ? html`<div class="chat-timestamp">${formatTime(msg.timestamp)}</div>`
        : nothing}
    </div>
  `;
}

// ============================================================================
// Tool Cards
// ============================================================================

type ToolCard = { name: string; args: string; result?: string };

function extractToolCards(msg: ChatMessage): ToolCard[] {
  const cards: ToolCard[] = [];
  for (const block of msg.content) {
    if (block.type === "toolcall" && block.name) {
      // Skip canvas tool - A2UI components are rendered separately
      if (block.name === "canvas") continue;
      cards.push({
        name: block.name,
        args: typeof block.args === "string" ? block.args : JSON.stringify(block.args || {}, null, 2),
        result: undefined,
      });
    }
    if (block.type === "toolresult" && block.result) {
      if (cards.length > 0 && !cards[cards.length - 1].result) {
        cards[cards.length - 1].result = typeof block.result === "string" ? block.result : JSON.stringify(block.result, null, 2);
      }
    }
  }
  // Fallback: toolName/toolCallId on message
  if (cards.length === 0 && msg.toolName) {
    // Skip canvas tool
    if (msg.toolName === "canvas") return cards;
    const text = msg.content.find((c) => c.type === "text" && c.text)?.text;
    cards.push({ name: msg.toolName, args: "{}", result: text });
  }
  return cards;
}

function renderToolCard(msg: ChatMessage): TemplateResult {
  const cards = extractToolCards(msg);
  return html`<div class="chat-tool-cards">${cards.map((tc) => renderToolCallCard(tc.name, tc.args, tc.result))}</div>`;
}

function renderToolCallCard(name: string, args: string, result?: string): TemplateResult {
  // Skip canvas tool - A2UI components are rendered separately
  if (name === "canvas") {
    return html``;
  }

  // Parse args for display
  let argsSummary = "";
  let hasArgs = false;
  try {
    const parsed = JSON.parse(args);
    hasArgs = Object.keys(parsed).length > 0;
    // Create a brief summary of args
    const keys = Object.keys(parsed);
    if (keys.length > 0) {
      argsSummary = keys.slice(0, 3).map(k => `${k}=${JSON.stringify(parsed[k]).slice(0, 20)}`).join(", ");
      if (keys.length > 3) argsSummary += "...";
    }
  } catch {
    hasArgs = args.trim().length > 0;
    argsSummary = args.slice(0, 50);
  }

  // Format result - truncate if too long
  let resultDisplay = result || "";
  const isResultJson = result && (result.startsWith("[") || result.startsWith("{"));
  if (result && result.length > 500) {
    if (isResultJson) {
      resultDisplay = result.slice(0, 500) + "... (点击展开查看全部)";
    }
  }

  return html`
    <details class="chat-tool-card">
      <summary class="chat-tool-card__header">
        <span class="chat-tool-card__icon">⚙</span>
        <span class="chat-tool-card__name">${name}</span>
        ${result
          ? html`<span class="chat-tool-card__status chat-tool-card__status--ok">✓</span>`
          : html`<span class="chat-tool-card__status chat-tool-card__status--running">⟳</span>`}
        ${hasArgs ? html`<span class="chat-tool-card__args-hint">${argsSummary}</span>` : nothing}
      </summary>
      <div class="chat-tool-card__body">
        ${hasArgs
          ? html`<div class="chat-tool-card__args-section">
              <div class="chat-tool-card__args-label">参数</div>
              <pre class="chat-tool-card__args">${args}</pre>
            </div>`
          : nothing}
        ${result
          ? html`<div class="chat-tool-card__result">${unsafeHTML(toMarkdownHtml(resultDisplay))}</div>`
          : html`<div class="chat-tool-card__result chat-tool-card__result--loading">等待结果...</div>`}
      </div>
    </details>
  `;
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
