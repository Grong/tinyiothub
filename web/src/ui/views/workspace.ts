import { LitElement, html, nothing } from "lit";
import { unsafeHTML } from "lit/directives/unsafe-html.js";
import { customElement, state } from "lit/decorators.js";
import { marked } from "marked";
import DOMPurify from "dompurify";
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

marked.setOptions({ async: false, gfm: true });

function md(text: string): string {
  try {
    return DOMPurify.sanitize(marked.parse(text) as string);
  } catch {
    return DOMPurify.sanitize(text);
  }
}

@customElement("view-workspace")
export class WorkspaceView extends LitElement {
  @state() chatState: ChatState = createChatState("", "");
  @state() draft: string = "";
  @state() agentId: string = "default";
  @state() private _showProcess = false;
  private a2uiRenderer = new A2uiRendererEngine(
    (functionId: string, data: Record<string, unknown>) => {
      this._handleA2uiAction(functionId, data);
    },
  );
  private _hadInsightData = false;
  private _expandedSections: Set<string> = new Set();

  private _toggleSection(key: string) {
    if (this._expandedSections.has(key)) {
      this._expandedSections.delete(key);
    } else {
      this._expandedSections.add(key);
    }
    this.requestUpdate();
  }

  private _isExpanded(key: string): boolean {
    return this._expandedSections.has(key);
  }

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
    this.chatState.onChange = () => this.requestUpdate();
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
  }

  render() {
    const stageIds = this.a2uiRenderer.getStageSurfaceIds();
    const dataIds = [
      ...this.a2uiRenderer.getInsightSurfaceIds(),
      ...this.a2uiRenderer.getInlineSurfaceIds(),
    ];
    const hasStage = stageIds.length > 0;
    const hasData = dataIds.length > 0;
    const insightEntering = hasData && !this._hadInsightData;
    if (insightEntering) this._hadInsightData = true;

    return html`
      <div class="workspace">
        <div class="workspace-header">
          <div class="workspace-header__title">工作空间</div>
          <button
            class="workspace-header__process-toggle${this._showProcess ? ' is-active' : ''}"
            @click=${() => { this._showProcess = !this._showProcess; }}
            title="AI 执行过程"
          >
            <svg viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round">
              <path d="M5 3v10M3 6l2-3 2 3M11 13V3M13 10l-2 3-2-3"/>
            </svg>
          </button>
        </div>

        <div class="workspace-stage">
          ${hasStage
            ? stageIds.map((id) => this.a2uiRenderer.renderSurface(id))
            : this._renderEmptyStage()}
        </div>

        ${this._showProcess
          ? html`<div class="workspace-process">
              <div class="workspace-process__header">
                <span class="workspace-process__header-title">执行过程</span>
              </div>
              <div class="workspace-process__body">
                ${this._renderProcessLog()}
              </div>
            </div>`
          : nothing}

        <div class="workspace-insight${insightEntering ? ' workspace-insight--enter' : ''}">
          <div class="workspace-insight__header">
            <span class="workspace-insight__header-title">数据洞察</span>
            ${this.chatState.chatSending
              ? html`<span class="workspace-insight__streaming-dot"></span>`
              : nothing}
          </div>
          <div class="workspace-insight__body">
            ${hasData ? this._renderInsightSurfaces(dataIds) : this._renderEmptyInsight()}
          </div>
        </div>

        <div class="workspace-compose">
          <textarea
            class="workspace-compose-input"
            .value=${this.draft}
            placeholder="描述物联网场景或数据..."
            ?disabled=${this.chatState.chatSending}
            @input=${(e: Event) => {
              this.draft = (e.target as HTMLTextAreaElement).value;
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
                <span class="workspace-compose-abort__spinner"></span>
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
      <div class="workspace-empty">
        <div class="workspace-empty__icon">
          <svg viewBox="0 0 48 48" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round">
            <path d="M24 4L6 14v20l18 10 18-10V14L24 4z"/>
            <path d="M6 14l18 10M24 24l18-10M24 24v20"/>
            <path d="M16 31l-10-5.5M32 31l10-5.5"/>
          </svg>
        </div>
        <p class="workspace-empty__title">3D 数字孪生场景</p>
        <p class="workspace-empty__hint">AI 将在此处生成可交互的 3D 场景</p>
        <div class="workspace-empty__prompts">
          <span>生成智能楼宇的 3D 模型</span>
          <span>可视化设备在楼层中的分布</span>
        </div>
      </div>
    `;
  }

  private _renderEmptyInsight() {
    return html`
      <div class="workspace-empty">
        <div class="workspace-empty__icon">
          <svg viewBox="0 0 48 48" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round">
            <rect x="4" y="4" width="40" height="40" rx="4"/>
            <path d="M4 20h40M16 20v24"/>
            <circle cx="32" cy="32" r="4"/>
            <path d="M36 28l4 4-4 4"/>
          </svg>
        </div>
        <p class="workspace-empty__title">数据洞察面板</p>
        <p class="workspace-empty__hint">AI 将通过对话为您填充设备状态、图表等数据卡片</p>
      </div>
    `;
  }

  private _extractMsgText(msg: ChatState["chatMessages"][number]): string {
    if (!Array.isArray(msg.content)) return "";
    return msg.content
      .filter((c) => c.type === "text" && typeof c.text === "string")
      .map((c) => c.text!)
      .join("");
  }

  private _renderCollapseChevron(expanded: boolean) {
    return html`
      <svg class="ws-chevron${expanded ? " is-expanded" : ""}" viewBox="0 0 16 16" fill="currentColor">
        <path d="M6 4l4 4-4 4" stroke="currentColor" stroke-width="1.5" fill="none" stroke-linecap="round" stroke-linejoin="round"/>
      </svg>
    `;
  }

  /** Extract all think blocks from a text string (handles multiple <think> tags). */
  private _extractThinkTags(text: string): { think: string[]; rest: string } {
    const think: string[] = [];
    let rest = text;
    // Use matchAll to capture ALL <think> and <thinking> tags
    const allMatches = [
      ...text.matchAll(/<think>([\s\S]*?)<\/think>/g),
      ...text.matchAll(/<thinking>([\s\S]*?)<\/thinking>/g),
    ];
    for (const m of allMatches) {
      if (m[1]?.trim()) think.push(m[1].trim());
    }
    rest = text.replace(/<think>[\s\S]*?<\/think>/g, "").replace(/<thinking>[\s\S]*?<\/thinking>/g, "").trim();
    return { think, rest };
  }

  /** Collect thinking blocks from a message's content. */
  private _collectThinking(msg: ChatState["chatMessages"][number]): string[] {
    const result: string[] = [];
    for (const block of msg.content) {
      if (block.type === "thinking" && typeof block.thinking === "string" && block.thinking.trim()) {
        result.push(block.thinking.trim());
      }
      if (block.type === "text" && block.text) {
        const { think } = this._extractThinkTags(block.text);
        result.push(...think);
      }
    }
    return result;
  }

  /** Collect visible text from a message's content (think tags stripped). */
  private _collectText(msg: ChatState["chatMessages"][number]): string[] {
    const result: string[] = [];
    for (const block of msg.content) {
      if (block.type === "text" && block.text) {
        const { rest } = this._extractThinkTags(block.text);
        if (rest) result.push(rest);
      }
    }
    return result;
  }

  private _renderThinkFold(key: string, thinkContents: string[]) {
    const thinkExpanded = this._isExpanded(key);
    return html`
      <div class="ws-think">
        <div class="ws-think__header" @click=${() => this._toggleSection(key)}>
          ${this._renderCollapseChevron(thinkExpanded)}
          <span class="ws-think__icon">
            <svg viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.5">
              <circle cx="8" cy="8" r="6"/>
              <path d="M8 5v4M8 11.5v.01"/>
            </svg>
          </span>
          <span class="ws-think__label">思考过程</span>
        </div>
        ${thinkExpanded ? html`
          <div class="ws-think__body">
            ${thinkContents.map((t) => html`<div class="ws-think__block">${unsafeHTML(md(t))}</div>`)}
          </div>
        ` : nothing}
      </div>
    `;
  }

  private _renderProcessLog() {
    const rows: ReturnType<typeof html>[] = [];
    let pendingThink: string[] = [];
    let thinkMsgIdx = 0; // Use index of first message in the think group as key

    const flushPendingThink = () => {
      if (pendingThink.length > 0) {
        rows.push(this._renderThinkFold(`think-${thinkMsgIdx}`, pendingThink));
        pendingThink = [];
      }
    };

    for (let mi = 0; mi < this.chatState.chatMessages.length; mi++) {
      const msg = this.chatState.chatMessages[mi];
      const role = (msg.role || "").toLowerCase();

      // ── User message ──
      if (role === "user") {
        flushPendingThink();
        const text = this._extractMsgText(msg);
        if (text) {
          rows.push(html`
            <div class="ws-msg ws-msg--user">
              <div class="ws-msg__bubble">${text}</div>
            </div>
          `);
        }
        continue;
      }

      // ── Tool message ──
      if (msg.toolCallId) {
        flushPendingThink();
        const key = `tool-${mi}`;
        const expanded = this._isExpanded(key);
        const toolName = msg.toolName || "";
        const toolCallBlock = msg.content.find((c) => c.type === "toolcall");
        const toolResultBlock = msg.content.find((c) => c.type === "toolresult");
        const args = toolCallBlock?.args ? JSON.stringify(toolCallBlock.args, null, 2) : "";
        const result = toolResultBlock?.result ? String(toolResultBlock.result) : "";
        const hasResult = !!result;

        rows.push(html`
          <div class="ws-tool ${hasResult ? "ws-tool--done" : "ws-tool--pending"}">
            <div class="ws-tool__header" @click=${() => this._toggleSection(key)}>
              ${this._renderCollapseChevron(expanded)}
              <span class="ws-tool__status-dot"></span>
              <span class="ws-tool__name">${toolName}</span>
              ${hasResult
                ? html`<span class="ws-tool__check">&#10003;</span>`
                : html`<span class="ws-tool__spinner"></span>`}
            </div>
            ${expanded ? html`
              <div class="ws-tool__body">
                ${args ? html`
                  <div class="ws-tool__section-label">参数</div>
                  <pre class="ws-tool__pre">${args}</pre>
                ` : nothing}
                ${result ? html`
                  <div class="ws-tool__section-label">结果</div>
                  <pre class="ws-tool__pre">${result}</pre>
                ` : nothing}
              </div>
            ` : nothing}
          </div>
        `);
        continue;
      }

      // ── AI message ──
      const msgThink = this._collectThinking(msg);
      const msgText = this._collectText(msg);

      if (msgText.length > 0) {
        // Message has visible text: flush pending + this message's thinking together, then text
        const allThink = [...pendingThink, ...msgThink];
        pendingThink = [];
        if (allThink.length > 0) {
          rows.push(this._renderThinkFold(`think-${thinkMsgIdx}`, allThink));
        }
        rows.push(html`
          <div class="ws-msg ws-msg--ai">
            <div class="ws-msg__text">${unsafeHTML(md(msgText.join("\n\n")))}</div>
          </div>
        `);
        thinkMsgIdx = mi + 1;
      } else if (msgThink.length > 0) {
        // Thinking-only message: accumulate
        if (pendingThink.length === 0) thinkMsgIdx = mi;
        pendingThink.push(...msgThink);
      }
      // else: empty message, skip
    }

    flushPendingThink();

    // ── In-progress tool calls (streaming) ──
    for (const id of this.chatState.toolStreamOrder) {
      const tc = this.chatState.toolStreamById.get(id);
      if (!tc) continue;
      const key = `tool-stream-${id}`;
      const expanded = this._isExpanded(key);
      const hasResult = !!tc.toolResult;

      rows.push(html`
        <div class="ws-tool ${hasResult ? "ws-tool--done" : "ws-tool--pending"}">
          <div class="ws-tool__header" @click=${() => this._toggleSection(key)}>
            ${this._renderCollapseChevron(expanded)}
            <span class="ws-tool__status-dot"></span>
            <span class="ws-tool__name">${tc.toolName}</span>
            ${hasResult
              ? html`<span class="ws-tool__check">&#10003;</span>`
              : html`<span class="ws-tool__spinner"></span>`}
          </div>
          ${expanded ? html`
            <div class="ws-tool__body">
              ${tc.toolArgs ? html`
                <div class="ws-tool__section-label">参数</div>
                <pre class="ws-tool__pre">${tc.toolArgs}</pre>
              ` : nothing}
              ${tc.toolResult ? html`
                <div class="ws-tool__section-label">结果</div>
                <pre class="ws-tool__pre">${tc.toolResult}</pre>
              ` : nothing}
            </div>
          ` : nothing}
        </div>
      `);
    }

    // ── Current streaming text ──
    if (this.chatState.chatStream?.trim()) {
      rows.push(html`
        <div class="ws-msg ws-msg--ai">
          <div class="ws-msg__text ws-msg__text--streaming">
            ${unsafeHTML(md(this.chatState.chatStream))}
            <span class="ws-msg__cursor"></span>
          </div>
        </div>
      `);
    }

    if (rows.length === 0) {
      return html`<div class="workspace-process__empty">暂无执行记录</div>`;
    }

    return html`<div class="ws-log">${rows}</div>`;
  }
}
