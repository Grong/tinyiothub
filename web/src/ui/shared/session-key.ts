/**
 * Shared session-key resolution and helpers.
 *
 * Chat view and Workspace view share the same session key logic.
 */
import { apiGet } from "../../api/client.js";

/**
 * Resolve (or create) a chat session key for the given agent.
 *
 * Persists the key in localStorage so chat history survives page reloads.
 * Regenerates when the workspace changes or the stored key is malformed.
 */
export async function resolveSessionKey(agentId: string): Promise<string> {
  let workspaceId = localStorage.getItem("workspace-id");

  if (!workspaceId) {
    // Fallback: fetch workspaces if not yet cached
    try {
      const wsRes = await apiGet<{ id: string; name: string }[]>("/workspaces");
      if (wsRes.result && wsRes.result.length > 0) {
        workspaceId = wsRes.result[0].id;
        localStorage.setItem("workspace-id", workspaceId);
      }
    } catch {
      // API unavailable — session key will use empty workspace
    }
  }

  const storedKey = localStorage.getItem("tinyiothub_chat_session_key");
  const storedWorkspace = storedKey?.split(":")[1];

  // Regenerate if missing, malformed, or workspace mismatch
  if (!storedKey || !storedKey.includes("/") || storedWorkspace !== workspaceId) {
    const ws = workspaceId || "";
    const newKey = `agent:${ws}:${agentId}/${crypto.randomUUID()}`;
    localStorage.setItem("tinyiothub_chat_session_key", newKey);
    return newKey;
  }

  return storedKey;
}
