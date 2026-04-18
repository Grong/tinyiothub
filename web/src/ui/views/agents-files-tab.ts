import { html } from "lit";
import { repeat } from "lit/directives/repeat.js";
import type { AgentsState } from "../controllers/agents.js";
import { apiPut } from "../../api/client.js";

export interface WorkspaceFile {
  name: string;
  content: string;
}

export interface WorkspaceFilesListResponse {
  files: { name: string }[];
}

const WORKSPACE_FILES = [
  "IDENTITY.md",
  "SOUL.md",
  "AGENTS.md",
  "USER.md",
  "TOOLS.md",
  "MEMORY.md",
  "HEARTBEAT.md",
  "BOOTSTRAP.md",
];

const FILE_DESCRIPTIONS: Record<string, string> = {
  "IDENTITY.md": "Agent 身份设定 - 名称、角色、基本信息",
  "SOUL.md": "Agent 灵魂设定 - 性格、语言风格、行为准则",
  "AGENTS.md": "多 Agent 协作配置 - 定义 Agent 间的关系和分工",
  "USER.md": "用户上下文 - 当前用户信息、偏好、权限",
  "TOOLS.md": "工具配置 - 可用工具列表和使用说明",
  "MEMORY.md": "记忆系统 - 知识库、向量检索配置",
  "HEARTBEAT.md": "心跳任务 - 周期性 IoT 巡检任务列表",
  "BOOTSTRAP.md": "启动引导 - Agent 初始化时的引导对话",
};

export function renderFilesTab(
  state: AgentsState,
  agentId: string,
  onUpdate: () => void,
) {
  const filesState = (state as any).workspaceFiles as Record<string, WorkspaceFile> | null;
  const loading = (state as any).workspaceFilesLoading as boolean | null;
  const error = (state as any).workspaceFilesError as string | null;
  const selectedFile = (state as any).selectedWorkspaceFile as string | null;
  const saving = (state as any).workspaceFilesSaving as boolean | null;

  if (loading && !filesState) {
    return html`
      <div class="files-tab">
        <div class="files-sidebar">
          ${[1, 2, 3, 4].map(() => html`<div class="files-skeleton skeleton-file-item"></div>`)}
        </div>
        <div class="files-editor">
          <div class="files-skeleton skeleton-editor"></div>
        </div>
      </div>
    `;
  }

  if (error && !filesState) {
    return html`
      <div class="files-tab">
        <div class="files-error">
          <svg class="files-error-icon" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.5">
            <path d="M12 9v4m0 4h.01M21 12a9 9 0 11-18 0 9 9 0 0118 0z"/>
          </svg>
          <span>${error}</span>
        </div>
      </div>
    `;
  }

  const currentFile = selectedFile || WORKSPACE_FILES[0];
  const fileContent = filesState?.[currentFile]?.content || "";

  return html`
    <div class="files-tab">
      <div class="files-sidebar">
        <div class="files-sidebar-header">
          <svg viewBox="0 0 20 20" fill="currentColor" width="14" height="14">
            <path fill-rule="evenodd" d="M4 4a2 2 0 012-2h4.586A2 2 0 0112 2.586L15.414 6A2 2 0 0116 7.414V16a2 2 0 01-2 2H6a2 2 0 01-2-2V4z" clip-rule="evenodd"/>
          </svg>
          <span>工作区文件</span>
        </div>
        <ul class="files-list">
          ${repeat(
            WORKSPACE_FILES,
            (name) => `file-${name}`,
            (name) => html`
              <li
                class="file-item ${currentFile === name ? 'active' : ''}"
                @click=${() => {
                  (state as any).selectedWorkspaceFile = name;
                  onUpdate();
                }}
              >
                <svg class="file-icon" viewBox="0 0 20 20" fill="currentColor" width="14" height="14">
                  <path fill-rule="evenodd" d="M4 4a2 2 0 012-2h4.586A2 2 0 0112 2.586L15.414 6A2 2 0 0116 7.414V16a2 2 0 01-2 2H6a2 2 0 01-2-2V4z" clip-rule="evenodd"/>
                </svg>
                <div class="file-info">
                  <span class="file-name">${name}</span>
                  <span class="file-desc">${FILE_DESCRIPTIONS[name] || ""}</span>
                </div>
              </li>
            `
          )}
        </ul>
      </div>

      <div class="files-editor">
        <div class="files-editor-header">
          <div class="files-editor-title">
            <svg viewBox="0 0 20 20" fill="currentColor" width="16" height="16">
              <path fill-rule="evenodd" d="M12.316 3.051a1 1 0 01.633 1.265l-4 12a1 1 0 11-1.898-.632l4-12a1 1 0 011.265-.633zM5.707 6.293a1 1 0 010 1.414L3.414 10l2.293 2.293a1 1 0 11-1.414 1.414l-3-3a1 1 0 010-1.414l3-3a1 1 0 011.414 0zm8.586 0a1 1 0 011.414 0l3 3a1 1 0 010 1.414l-3 3a1 1 0 11-1.414-1.414L16.586 10l-2.293-2.293a1 1 0 010-1.414z" clip-rule="evenodd"/>
            </svg>
            <span>${currentFile}</span>
          </div>
          <button
            class="files-save-btn ${saving ? 'saving' : ''}"
            ?disabled=${saving}
            @click=${async () => {
              if (!agentId || saving) return;
              (state as any).workspaceFilesSaving = true;
              onUpdate();
              try {
                await apiPut(`/agents/${agentId}/files/${currentFile}`, {
                  content: fileContent,
                });
                // Update local state to reflect saved content
                if ((state as any).workspaceFiles) {
                  (state as any).workspaceFiles[currentFile] = {
                    name: currentFile,
                    content: fileContent,
                  };
                }
              } catch (err) {
                alert("保存失败: " + String(err));
              } finally {
                (state as any).workspaceFilesSaving = false;
                onUpdate();
              }
            }}
          >
            ${saving
              ? html`<svg class="spin" viewBox="0 0 20 20" fill="currentColor" width="14" height="14">
                  <path fill-rule="evenodd" d="M4 2a1 1 0 011 1v2.101a7.002 7.002 0 0111.601 2.566 1 1 0 11-1.885.666A5.002 5.002 0 005.999 7H9a1 1 0 010 2H4a1 1 0 01-1-1V3a1 1 0 011-1zm.008 9.057a1 1 0 011.276.61A5.002 5.002 0 0014.001 13H11a1 1 0 110-2h5a1 1 0 011 1v5a1 1 0 11-2 0v-2.101a7.002 7.002 0 01-11.601-2.566 1 1 0 01.61-1.276z" clip-rule="evenodd"/>
                </svg> 保存中...`
              : html`<svg viewBox="0 0 20 20" fill="currentColor" width="14" height="14">
                  <path d="M3 4a1 1 0 011-1h8a1 1 0 011 1v12a1 1 0 01-1 1H4a1 1 0 01-1-1V4z"/>
                  <path fill-rule="evenodd" d="M9.502 5.513a.5.5 0 00-.5-.5H4a1 1 0 010-2h5.002a.5.5 0 00.5.5z" clip-rule="evenodd"/>
                  <path d="M6.5 10a.5.5 0 01.5-.5h5a.5.5 0 01.5.5v3a.5.5 0 01-.5.5H7a.5.5 0 01-.5-.5v-3z"/>
                </svg> 保存`
            }
          </button>
        </div>
        <textarea
          class="files-textarea"
          .value=${fileContent}
          placeholder="在此编辑文件内容..."
          spellcheck="false"
          @input=${(e: Event) => {
            const newContent = (e.target as HTMLTextAreaElement).value;
            if ((state as any).workspaceFiles) {
              (state as any).workspaceFiles[currentFile] = {
                name: currentFile,
                content: newContent,
              };
            }
            // Auto-save on change could be implemented here
          }}
        ></textarea>
      </div>
    </div>
  `;
}
