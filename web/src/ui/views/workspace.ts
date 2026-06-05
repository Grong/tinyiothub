import { LitElement, html, nothing } from "lit";
import { customElement, state } from "lit/decorators.js";
import type { ChatState } from "../controllers/chat.js";
import {
  createChatState,
  loadChatHistory,
  sendChatMessage,
  abortChatRun,
} from "../controllers/chat.js";
import { apiGet } from "../../api/client.js";
import { A2uiRendererEngine } from "../chat/a2ui/a2ui-renderer.js";
import "../../styles/views/workspace.css";

@customElement("view-workspace")
export class WorkspaceView extends LitElement {
  @state() chatState: ChatState = createChatState("", "");
  @state() draft: string = "";
  @state() agentId: string = "default";

  private pollTimer: ReturnType<typeof setInterval> | null = null;
  private a2uiRenderer = new A2uiRendererEngine(
    (functionId: string, data: Record<string, unknown>) => {
      this._handleA2uiAction(functionId, data);
    },
  );

  createRenderRoot() {
    return this;
  }

  async connectedCallback(): Promise<void> {
    super.connectedCallback();
    this.agentId = "default";

    let workspaceId = localStorage.getItem("workspace-id");
    if (!workspaceId) {
      try {
        const wsRes = await apiGet<{ id: string; name: string }[]>("/workspaces");
        if (wsRes.result && wsRes.result.length > 0) {
          workspaceId = wsRes.result[0].id;
          localStorage.setItem("workspace-id", workspaceId);
        }
      } catch {
        // ignore
      }
    }

    const storedKey = localStorage.getItem("tinyiothub_chat_session_key");
    let sessionKey = storedKey;
    const storedWorkspace = storedKey?.split(":")[1];
    if (!storedKey || !storedKey.includes("/") || storedWorkspace !== workspaceId) {
      const ws = workspaceId || "";
      sessionKey = `agent:${ws}:${this.agentId}/${crypto.randomUUID()}`;
      localStorage.setItem("tinyiothub_chat_session_key", sessionKey);
    }

    this.chatState = createChatState(sessionKey || "", this.agentId);
    this._bindA2uiCallback();
    await loadChatHistory(this.chatState);

    for (const msg of this.chatState.chatMessages) {
      const a2ui = (msg as Record<string, unknown>).a2ui as string | undefined;
      if (a2ui) {
        this.a2uiRenderer.handleA2uiMessage(a2ui);
      }
    }
    this.requestUpdate();
  }

  disconnectedCallback(): void {
    super.disconnectedCallback();
    this._stopPolling();
  }

  private _bindA2uiCallback(): void {
    this.chatState.onA2ui = (jsonl: string) => {
      this.a2uiRenderer.handleA2uiMessage(jsonl);
      this.requestUpdate();
    };
  }

  private _handleSend(): void {
    const raw = this.draft.trim();
    if (!raw) return;
    this.draft = "";

    this.a2uiRenderer.clear();
    this.requestUpdate();

    const contextualMsg = `[当前页面：Workspace 工作空间]\n${raw}`;

    sendChatMessage(this.chatState, contextualMsg);
    this._startPolling();
  }

  private _handleAbort(): void {
    abortChatRun(this.chatState);
  }

  private _handleA2uiAction(functionId: string, data: Record<string, unknown>): void {
    const deviceId = data.deviceId as string | undefined;

    if (functionId === "viewDevice" && deviceId) {
      window.history.pushState({}, "", `/devices/${deviceId}`);
      window.dispatchEvent(new PopStateEvent("popstate"));
      return;
    }
    if (functionId === "controlDevice" && deviceId) {
      window.history.pushState({}, "", `/devices/${deviceId}`);
      window.dispatchEvent(new PopStateEvent("popstate"));
      return;
    }

    const actionMsg = `[操作] ${functionId}: ${JSON.stringify(data)}`;
    sendChatMessage(this.chatState, actionMsg);
    this._startPolling();
  }

  private _startPolling(): void {
    this._stopPolling();
    this.pollTimer = setInterval(() => {
      this.requestUpdate();
      if (!this.chatState.chatSending) {
        this._stopPolling();
      }
    }, 100);
  }

  private _stopPolling(): void {
    if (this.pollTimer) {
      clearInterval(this.pollTimer);
      this.pollTimer = null;
    }
  }

  render() {
    const stageIds = this.a2uiRenderer.getStageSurfaceIds();
    const dataIds = [
      ...this.a2uiRenderer.getInsightSurfaceIds(),
      ...this.a2uiRenderer.getInlineSurfaceIds(),
    ];
    const hasStage = stageIds.length > 0;
    const hasData = dataIds.length > 0;

    return html`
      <div class="workspace">
        <div class="workspace-header">
          <div class="workspace-header__title">工作空间</div>
        </div>

        <!-- 3D Stage — full bleed background -->
        <div class="workspace-stage">
          ${hasStage
            ? stageIds.map((id) => this.a2uiRenderer.renderSurface(id))
            : this._renderEmptyStage()}
        </div>

        <!-- Data Panel — floating overlay -->
        <div class="workspace-insight">
          ${hasData ? this._renderInsightSurfaces(dataIds) : this._renderEmptyInsight()}
        </div>

        <!-- Compose — floating bottom bar -->
        <div class="workspace-compose">
          <textarea
            class="workspace-compose-input"
            .value=${this.draft}
            placeholder="描述您想分析的物联网场景或数据..."
            ?disabled=${this.chatState.chatSending}
            @input=${(e: Event) => {
              const ta = e.target as HTMLTextAreaElement;
              this.draft = ta.value;
              ta.style.height = "auto";
              ta.style.height = Math.min(ta.scrollHeight, 120) + "px";
            }}
            @keydown=${(e: KeyboardEvent) => {
              if (e.key === "Enter" && !e.shiftKey) {
                e.preventDefault();
                this._handleSend();
              }
            }}
          ></textarea>
          ${this.chatState.chatSending
            ? html`<button class="workspace-compose-abort" @click=${this._handleAbort}>
                停止
              </button>`
            : html`<button
                class="workspace-compose-send"
                @click=${this._handleSend}
                ?disabled=${!this.draft.trim()}
              >
                发送
              </button>`}
        </div>
      </div>
    `;
  }

  private _renderInsightSurfaces(dataIds: string[]) {
    const BLOCK_KINDS = ["Table", "DeviceTable", "AlarmTable", "List", "Tabs", "Chart", "DataChart", "Modal", "Column", "Scene3D"];
    const groups: { type: "stat-row" | "single"; ids: string[] }[] = [];

    for (const id of dataIds) {
      let kinds: string[];
      try {
        kinds = this.a2uiRenderer.getSurfaceComponentKinds(id);
      } catch {
        kinds = [];
      }

      const isBlock = kinds.some((k) => BLOCK_KINDS.includes(k));
      const isSimple = kinds.length === 0 || !isBlock;

      const last = groups[groups.length - 1];
      if (isSimple && last?.type === "stat-row") {
        last.ids.push(id);
      } else if (isSimple) {
        groups.push({ type: "stat-row", ids: [id] });
      } else {
        groups.push({ type: "single", ids: [id] });
      }
    }

    return groups.map((group) => {
      if (group.type === "stat-row" && group.ids.length > 1) {
        return html`
          <div class="a2ui-stat-row a2ui-stat-row--merged">
            ${group.ids.map((id) => this.a2uiRenderer.renderSurface(id))}
          </div>
        `;
      }
      return this.a2uiRenderer.renderSurface(group.ids[0]);
    });
  }

  private _renderEmptyStage() {
    return html`
      <div class="workspace-empty">3D 场景区域</div>
    `;
  }

  private _renderEmptyInsight() {
    return html`
      <div class="workspace-empty">数据洞察面板</div>
    `;
  }
}
