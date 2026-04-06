export type NormalizedMessage = {
  role: string;
  content: NormalizedContentItem[];
  timestamp: number;
  id?: string;
  senderLabel?: string | null;
};

export type NormalizedContentItem = {
  type: string;
  text?: string;
  name?: string;
  args?: string;
};

export function normalizeMessage(message: unknown): NormalizedMessage {
  const m = message as Record<string, unknown>;
  let role = typeof m.role === "string" ? m.role : "unknown";

  // Detect tool messages
  const hasToolId = typeof m.toolCallId === "string" || typeof m.tool_call_id === "string";
  const hasToolName = typeof m.toolName === "string" || typeof m.tool_name === "string";
  const contentRaw = m.content;
  const contentItems = Array.isArray(contentRaw) ? contentRaw : null;
  const hasToolContent =
    Array.isArray(contentItems) &&
    contentItems.some((item) => {
      const x = item as Record<string, unknown>;
      const t = (x.type as string) || "";
      return t.startsWith("tool_") || t === "tool_result" || t === "tool_use";
    });

  if (hasToolId || hasToolContent || hasToolName) {
    role = "toolResult";
  }

  let content: NormalizedContentItem[] = [];

  if (typeof m.content === "string") {
    content = [{ type: "text", text: m.content }];
  } else if (Array.isArray(m.content)) {
    content = m.content.map((item: Record<string, unknown>) => ({
      type: (item.type as string) || "text",
      text: item.text as string | undefined,
      name: item.name as string | undefined,
      args: typeof item.args === "string" ? item.args : item.args ? JSON.stringify(item.args) : undefined,
    }));
  } else if (typeof m.text === "string") {
    content = [{ type: "text", text: m.text }];
  }

  const timestamp = typeof m.timestamp === "number" ? m.timestamp : Date.now();
  const id = typeof m.id === "string" ? m.id : undefined;
  const senderLabel =
    typeof m.senderLabel === "string" && m.senderLabel.trim() ? m.senderLabel.trim() : null;

  return { role, content, timestamp, id, senderLabel };
}

export function normalizeRoleForGrouping(role: string): string {
  const lower = role.toLowerCase();
  if (lower === "assistant" || lower === "model") return "assistant";
  if (lower === "user") return "user";
  if (lower === "toolresult" || lower === "tool_result") return "tool";
  return lower;
}

export function isToolResultMessage(message: NormalizedMessage): boolean {
  return message.role === "toolResult" || message.role === "tool";
}
