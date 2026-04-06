import { html, nothing, type TemplateResult } from "lit";
import { unsafeHTML } from "lit/directives/unsafe-html.js";
import { marked } from "marked";
import DOMPurify from "dompurify";
import type { NormalizedMessage } from "./message-normalizer.js";
import { normalizeMessage, normalizeRoleForGrouping } from "./message-normalizer.js";

export type MessageGroup = {
  role: string;
  messages: NormalizedMessage[];
  firstTimestamp: number;
};

// Configure marked with highlight.js
marked.setOptions({
  async: false,
  gfm: true,
});

function toMarkdownHtml(text: string): string {
  const raw = marked.parse(text) as string;
  return DOMPurify.sanitize(raw);
}

export function groupMessages(messages: unknown[]): MessageGroup[] {
  const groups: MessageGroup[] = [];
  let currentGroup: MessageGroup | null = null;

  for (const raw of messages) {
    const msg = normalizeMessage(raw);
    const normalizedRole = normalizeRoleForGrouping(msg.role);

    if (currentGroup && currentGroup.role === normalizedRole) {
      currentGroup.messages.push(msg);
    } else {
      currentGroup = {
        role: normalizedRole,
        messages: [msg],
        firstTimestamp: msg.timestamp,
      };
      groups.push(currentGroup);
    }
  }

  return groups;
}

export function renderMessageGroup(group: MessageGroup): TemplateResult {
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
        ${group.messages.map((msg) => renderSingleMessage(msg, isTool))}
      </div>
    </div>
  `;
}

function renderSingleMessage(
  msg: NormalizedMessage,
  isTool: boolean,
): TemplateResult {
  if (isTool) {
    return renderToolMessage(msg);
  }

  return html`
    <div
      class="chat-bubble ${msg.role === "user"
        ? "chat-bubble--user"
        : "chat-bubble--assistant"}"
    >
      ${msg.content.map((item) => {
        if (item.type === "text" && item.text) {
          return html`<div class="chat-content">
            ${unsafeHTML(toMarkdownHtml(item.text))}
          </div>`;
        }
        return nothing;
      })}
      ${msg.timestamp
        ? html`
            <div class="chat-timestamp">${formatTime(msg.timestamp)}</div>
          `
        : nothing}
    </div>
  `;
}

function renderToolMessage(msg: NormalizedMessage): TemplateResult {
  const toolName = msg.content.find((c) => c.name)?.name || "Tool";
  const args = msg.content.find((c) => c.args)?.args;
  const text = msg.content.find((c) => c.type === "text" && c.text)?.text;

  return html`
    <div class="chat-tool-card">
      <div class="chat-tool-card__header">
        <span class="chat-tool-card__icon">
          <svg
            viewBox="0 0 24 24"
            fill="none"
            stroke="currentColor"
            stroke-width="2"
            width="14"
            height="14"
          >
            <path
              d="M14.7 6.3a1 1 0 0 0 0 1.4l1.6 1.6a1 1 0 0 0 1.4 0l3.77-3.77a6 6 0 0 1-7.94 7.94l-6.91 6.91a2.12 2.12 0 0 1-3-3l6.91-6.91a6 6 0 0 1 7.94-7.94l-3.76 3.76z"
            />
          </svg>
        </span>
        <span class="chat-tool-card__name">${toolName}</span>
      </div>
      ${args
        ? html`<pre class="chat-tool-card__args">${args}</pre>`
        : nothing}
      ${text
        ? html`<div class="chat-tool-card__result">
            ${unsafeHTML(toMarkdownHtml(text))}
          </div>`
        : nothing}
    </div>
  `;
}

export function renderStreamingGroup(
  text: string,
  startedAt: number,
): TemplateResult {
  return html`
    <div class="chat-group assistant">
      <div class="chat-avatar chat-avatar--assistant">A</div>
      <div class="chat-group-messages">
        <div class="chat-bubble chat-bubble--assistant">
          <div class="chat-content">${unsafeHTML(toMarkdownHtml(text))}</div>
          <div class="chat-streaming-indicator" aria-hidden="true">
            <span></span><span></span><span></span>
          </div>
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

function formatTime(timestamp: number): string {
  return new Date(timestamp).toLocaleTimeString([], {
    hour: "numeric",
    minute: "2-digit",
  });
}
