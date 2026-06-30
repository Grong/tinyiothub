/**
 * Secure SSE client — uses fetch + ReadableStream instead of EventSource.
 *
 * Unlike EventSource (which cannot set custom headers), this client sends the
 * auth token via the Authorization header, keeping it out of URLs, browser
 * history, and server access logs.
 */

import { getAuthToken, buildUrl } from "./client.js";

export type SseEventHandler = (event: string, data: string) => void;

export interface SseConnection {
  /** Close the connection. Safe to call multiple times. */
  close(): void;
  /** Whether the connection is still open. */
  readonly closed: boolean;
}

/**
 * Open a secure SSE connection with automatic reconnection.
 *
 * @param endpoint  — API path (e.g. "/workspaces/notifications/stream")
 * @param onEvent   — called for each received SSE event
 * @param onError   — called when the connection errors (auto-reconnect is built-in)
 * @param signal    — optional AbortSignal for external cancellation
 */
export function connectSse(
  endpoint: string,
  onEvent: SseEventHandler,
  onError?: (err: Error) => void,
  signal?: AbortSignal,
): SseConnection {
  let closed = false;
  const controller = new AbortController();

  // Wire external signal to internal controller
  if (signal) {
    if (signal.aborted) {
      return { close() {}, closed: true };
    }
    signal.addEventListener(
      "abort",
      () => {
        closed = true;
        controller.abort();
      },
      { once: true },
    );
  }

  const headers: Record<string, string> = { Accept: "text/event-stream" };
  const token = getAuthToken();
  if (token) headers["Authorization"] = `Bearer ${token}`;

  async function connect() {
    while (!closed) {
      try {
        const response = await fetch(buildUrl(endpoint), {
          headers,
          signal: controller.signal,
          credentials: "include",
        });

        if (!response.ok || !response.body) {
          if (!closed) onError?.(new Error(`SSE connection failed: HTTP ${response.status}`));
          return;
        }

        const reader = response.body.getReader();
        const decoder = new TextDecoder();
        let lineBuffer = ""; // partial lines from stream
        let dataBuffer = ""; // accumulated data lines for current event
        let eventType = "message";

        while (!closed) {
          const { done, value } = await reader.read();
          if (done) break;

          lineBuffer += decoder.decode(value, { stream: true });
          const lines = lineBuffer.split("\n");
          lineBuffer = lines.pop() || ""; // keep incomplete last line

          for (const line of lines) {
            const trimmed = line.endsWith("\r") ? line.slice(0, -1) : line;

            // Empty line = dispatch event
            if (trimmed === "") {
              if (dataBuffer) {
                onEvent(eventType, dataBuffer);
              }
              dataBuffer = "";
              eventType = "message";
              continue;
            }

            // Comment line (SSE spec)
            if (trimmed.startsWith(":")) continue;

            // event: field
            if (trimmed.startsWith("event: ")) {
              eventType = trimmed.slice(7).trim();
              continue;
            }

            // data: field (leading space is optional per spec)
            if (trimmed.startsWith("data:")) {
              const payload = trimmed.slice(5).replace(/^ /, "");
              dataBuffer = dataBuffer ? dataBuffer + "\n" + payload : payload;
              continue;
            }

            // id: or retry: fields — ignored for now
            if (trimmed.startsWith("id:") || trimmed.startsWith("retry:")) continue;

            // Any other line treated as unnamed data
            dataBuffer = dataBuffer ? dataBuffer + "\n" + trimmed : trimmed;
          }
        }

        // Flush any remaining event when stream ends gracefully
        if (dataBuffer && !closed) {
          onEvent(eventType, dataBuffer);
        }
      } catch (err) {
        if ((err as Error).name === "AbortError") break;
        if (!closed) onError?.(err as Error);
      }

      // Reconnect after a delay (only if not intentionally closed)
      if (!closed) {
        await new Promise((r) => setTimeout(r, 3000));
      }
    }
  }

  connect();

  return {
    close() {
      closed = true;
      controller.abort();
    },
    get closed() {
      return closed;
    },
  };
}
