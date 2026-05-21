import { html } from "lit";
import { apiGet, apiPut } from "../../api/client.js";

interface WorkspaceFileContent {
  name: string;
  content: string;
}

interface TabState {
  content: string;
  loading: boolean;
  saving: boolean;
  dirty: boolean;
  error: string;
  loadedAgentId: string | null;
}

const tabStates = new Map<string, TabState>();

function getTabState(agentId: string): TabState {
  let s = tabStates.get(agentId);
  if (!s || s.loadedAgentId !== agentId) {
    s = { content: "", loading: true, saving: false, dirty: false, error: "", loadedAgentId: agentId };
    tabStates.set(agentId, s);
  }
  return s;
}

async function loadWorkspaceDescription(agentId: string): Promise<string> {
  try {
    const res = await apiGet<WorkspaceFileContent>(`/agents/${agentId}/files/USER.md`);
    return res.result?.content || "";
  } catch {
    return "";
  }
}

async function saveWorkspaceDescription(
  agentId: string,
  content: string,
): Promise<void> {
  await apiPut(`/agents/${agentId}/files/USER.md`, { content });
}

export function renderWorkspaceTab(
  agentId: string,
  onUpdate: () => void,
): ReturnType<typeof html> {
  const state = getTabState(agentId);

  if (state.loading) {
    loadWorkspaceDescription(agentId).then((content) => {
      state.content = content;
      state.loading = false;
      state.dirty = false;
      onUpdate();
    });
    return html`<div class="callout info">加载中...</div>`;
  }

  return html`
    <section class="card">
      <div class="card-title">工作区设定</div>
      <div class="card-sub">描述这个 AI 助手所管理的工作区背景</div>

      <div class="field" style="margin-top: 16px;">
        <textarea
          class="textarea"
          rows="8"
          placeholder="描述这个 AI 助手所管理的工作区背景，例如：所管理的园区、建筑、设备类型和数量..."
          .value=${state.content}
          @input=${(e: InputEvent) => {
            state.content = (e.target as HTMLTextAreaElement).value;
            state.dirty = true;
          }}
        ></textarea>
      </div>

      <div style="display: flex; gap: 8px; margin-top: 16px;">
        <button
          type="button"
          class="btn btn--sm primary"
          ?disabled=${!state.dirty || state.saving}
          @click=${async () => {
            state.saving = true;
            state.error = "";
            onUpdate();
            try {
              await saveWorkspaceDescription(agentId, state.content);
              state.dirty = false;
            } catch (err) {
              state.error = err instanceof Error ? err.message : String(err);
            } finally {
              state.saving = false;
              onUpdate();
            }
          }}
        >
          ${state.saving ? "保存中..." : "保存"}
        </button>
      </div>

      ${state.error
        ? html`<div class="callout warn" style="margin-top: 8px;">保存失败: ${state.error}</div>`
        : ""}
    </section>
  `;
}
