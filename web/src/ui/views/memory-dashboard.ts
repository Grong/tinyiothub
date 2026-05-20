import { LitElement, html } from "lit";
import { customElement, state } from "lit/decorators.js";
import { listActiveMemories, getPendingQueue, resolveQueueItem, pinMemory } from "../../api/memory";
import type { AgentMemory, ReflectionQueueItem } from "../../api/memory";
import { apiGet } from "../../api/client.js";

const ZONE_LABELS: Record<string, string> = {
  core: "核心",
  work: "工作",
  episode: "会话",
  general: "通用",
};

const SOURCE_LABELS: Record<string, string> = {
  user: "用户",
  reflection: "反思",
  import: "导入",
  system: "系统",
  deviceSnapshot: "设备快照",
};

const CONFIDENCE_LABELS: Record<string, string> = {
  high: "高",
  medium: "中",
  low: "低",
};

@customElement("view-memory-dashboard")
export class ViewMemoryDashboard extends LitElement {
  @state() private activeTab: "memories" | "queue" | "audit" = "memories";
  @state() private memories: AgentMemory[] = [];
  @state() private queue: ReflectionQueueItem[] = [];
  @state() private workspaceId = "";
  @state() private agentId = "";
  @state() private loading = false;
  @state() private error: string | null = null;

  createRenderRoot() {
    return this;
  }

  connectedCallback(): void {
    super.connectedCallback();
    const params = new URLSearchParams(window.location.search);
    this.workspaceId = params.get("workspace") || localStorage.getItem("workspace-id") || "";
    this.agentId = params.get("agent") || "default";
    this.init();
  }

  private async init() {
    if (!this.workspaceId) {
      try {
        const wsRes = await apiGet<{ id: string; name: string }[]>('/workspaces');
        if (wsRes.result && wsRes.result.length > 0) {
          this.workspaceId = wsRes.result[0].id;
          localStorage.setItem("workspace-id", this.workspaceId);
        }
      } catch {
        // API failed — will show error
      }
    }
    this.loadData();
  }

  private async loadData() {
    if (!this.workspaceId || !this.agentId) {
      this.error = "缺少 workspace 或 agent 参数";
      return;
    }
    this.loading = true;
    this.error = null;
    try {
      if (this.activeTab === "memories") {
        const res = await listActiveMemories(this.workspaceId, this.agentId);
        this.memories = res.result || [];
      } else if (this.activeTab === "queue") {
        const res = await getPendingQueue(this.workspaceId, this.agentId);
        this.queue = res.result || [];
      }
    } catch (e: any) {
      this.error = e.message || "加载失败";
    } finally {
      this.loading = false;
    }
  }

  private async handleResolve(queueId: string, approved: boolean) {
    try {
      await resolveQueueItem(this.workspaceId, queueId, approved);
      await this.loadData();
    } catch (e: any) {
      this.error = e.message || "操作失败";
    }
  }

  private async handlePin(memoryId: string, currentlyPinned: boolean) {
    try {
      await pinMemory(this.workspaceId, memoryId, !currentlyPinned);
      await this.loadData();
    } catch (e: any) {
      this.error = e.message || "操作失败";
    }
  }

  private switchTab(tab: "memories" | "queue" | "audit") {
    this.activeTab = tab;
    this.loadData();
  }

  render() {
    if (this.error && this.memories.length === 0 && this.queue.length === 0) {
      return html`<div class="empty-state">${this.error}</div>`;
    }

    return html`
      <div class="memory-dashboard">
        <div class="md-tabs">
          <button
            class="md-tab ${this.activeTab === "memories" ? "active" : ""}"
            @click=${() => this.switchTab("memories")}
          >活跃记忆</button>
          <button
            class="md-tab ${this.activeTab === "queue" ? "active" : ""}"
            @click=${() => this.switchTab("queue")}
          >审核队列 ${this.queue.length > 0 ? html`(${this.queue.length})` : ""}</button>
          <button
            class="md-tab ${this.activeTab === "audit" ? "active" : ""}"
            @click=${() => this.switchTab("audit")}
          >审计日志</button>
        </div>

        ${this.loading
          ? html`<div class="md-loading">加载中…</div>`
          : this.activeTab === "memories"
            ? this.renderMemories()
            : this.activeTab === "queue"
              ? this.renderQueue()
              : this.renderAudit()}
      </div>
    `;
  }

  private renderMemories() {
    if (this.memories.length === 0) {
      return html`<div class="empty-state">暂无活跃记忆</div>`;
    }
    return html`
      <div class="md-list">
        ${this.memories.map(
          (m) => html`
            <div class="md-card ${m.pinned ? "pinned" : ""}">
              <div class="md-card-header">
                <span class="md-badge zone-${m.zone}">${ZONE_LABELS[m.zone] || m.zone}</span>
                <span class="md-badge source-${m.source}">${SOURCE_LABELS[m.source] || m.source}</span>
                <span class="md-confidence">置信度: ${CONFIDENCE_LABELS[m.confidence] || m.confidence}</span>
                <span class="md-effectiveness">有效: ${(m.effectiveness * 100).toFixed(0)}%</span>
                ${m.pinned ? html`<span class="md-pin-badge">已置顶</span>` : ""}
              </div>
              <div class="md-card-body">${m.content}</div>
              <div class="md-card-footer">
                <button
                  class="btn btn--sm ${m.pinned ? "btn--outline" : "btn--primary"}"
                  @click=${() => this.handlePin(m.id, m.pinned)}
                >${m.pinned ? "取消置顶" : "置顶"}</button>
              </div>
            </div>
          `,
        )}
      </div>
    `;
  }

  private renderQueue() {
    if (this.queue.length === 0) {
      return html`<div class="empty-state">暂无待审核项</div>`;
    }
    return html`
      <div class="md-list">
        ${this.queue.map((item) => {
          let data: any = {};
          try {
            data = JSON.parse(item.candidateData);
          } catch {
            // use raw data
          }
          return html`
            <div class="md-card">
              <div class="md-card-header">
                <span class="md-badge type-${item.candidateType}">${item.candidateType === "memory" ? "记忆" : "技能"}</span>
                <span class="md-time">${item.createdAt?.slice(0, 16) || ""}</span>
              </div>
              <div class="md-card-body">
                ${data.fact || data.name || data.description || item.candidateData}
              </div>
              ${data.reasoning
                ? html`<div class="md-reasoning">理由: ${data.reasoning}</div>`
                : ""}
              <div class="md-card-footer">
                <button
                  class="btn btn--sm btn--success"
                  @click=${() => this.handleResolve(item.id, true)}
                >通过</button>
                <button
                  class="btn btn--sm btn--danger"
                  @click=${() => this.handleResolve(item.id, false)}
                >拒绝</button>
              </div>
            </div>
          `;
        })}
      </div>
    `;
  }

  private renderAudit() {
    return html`<div class="empty-state">审计日志功能即将推出</div>`;
  }
}
