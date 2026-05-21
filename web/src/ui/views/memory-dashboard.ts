import { LitElement, html } from "lit";
import { customElement, state } from "lit/decorators.js";
import { listActiveMemories, getPendingQueue, resolveQueueItem, pinMemory, compileProfile, generateWeeklyDigest } from "../../api/memory";
import type { AgentMemory, ReflectionQueueItem } from "../../api/memory";
import { getAuthToken } from "../../api/client";

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
  @state() private agentId = "";
  @state() private loading = false;
  @state() private error: string | null = null;
  @state() private notifications: string[] = [];
  private _sseSource: EventSource | null = null;

  createRenderRoot() {
    return this;
  }

  connectedCallback(): void {
    super.connectedCallback();
    const params = new URLSearchParams(window.location.search);
    this.agentId = params.get("agent") || "default";
    this.loadData();
    this._connectSSE();
  }

  disconnectedCallback(): void {
    super.disconnectedCallback();
    if (this._sseSource) {
      this._sseSource.close();
      this._sseSource = null;
    }
  }

  private _connectSSE() {
    if (this._sseSource) this._sseSource.close();
    const token = getAuthToken();
    this._sseSource = new EventSource(`/api/v1/workspaces/notifications/stream?token=${encodeURIComponent(token || "")}`);
    this._sseSource.addEventListener("skill_notification", (event) => {
      try {
        const data = JSON.parse(event.data);
        this.notifications = [...this.notifications, data.message];
        this.requestUpdate();
        setTimeout(() => {
          this.notifications = this.notifications.filter((n) => n !== data.message);
          this.requestUpdate();
        }, 8000);
      } catch {
        // ignore malformed events
      }
    });
    this._sseSource.onerror = () => {
      setTimeout(() => {
        if (this._sseSource?.readyState === EventSource.CLOSED) {
          this._connectSSE();
        }
      }, 3000);
    };
  }

  private async loadData() {
    if (!this.agentId) {
      this.error = "缺少 agent 参数";
      return;
    }
    this.loading = true;
    this.error = null;
    try {
      if (this.activeTab === "memories") {
        const res = await listActiveMemories(this.agentId);
        this.memories = res.result || [];
      } else if (this.activeTab === "queue") {
        const res = await getPendingQueue(this.agentId);
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
      await resolveQueueItem(queueId, approved);
      await this.loadData();
    } catch (e: any) {
      this.error = e.message || "操作失败";
    }
  }

  private async handlePin(memoryId: string, currentlyPinned: boolean) {
    try {
      await pinMemory(memoryId, !currentlyPinned);
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
      return html`<div class="memory-empty">${this.error}</div>`;
    }

    return html`
      <div>
        ${this.notifications.length > 0
          ? html`<div class="memory-notifications">
              ${this.notifications.map(
                (msg) => html`<div class="memory-notification">${msg}</div>`,
              )}
            </div>`
          : ""}
        <div class="detail-tabs">
          <button
            class="detail-tab ${this.activeTab === "memories" ? "active" : ""}"
            @click=${() => this.switchTab("memories")}
          >活跃记忆</button>
          <button
            class="detail-tab ${this.activeTab === "queue" ? "active" : ""}"
            @click=${() => this.switchTab("queue")}
          >审核队列 ${this.queue.length > 0 ? html`<span class="chip">${this.queue.length}</span>` : ""}</button>
          <button
            class="detail-tab ${this.activeTab === "audit" ? "active" : ""}"
            @click=${() => this.switchTab("audit")}
          >审计日志</button>
        </div>

        ${this.loading
          ? html`<div class="memory-loading">加载中…</div>`
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
      return html`<div class="memory-empty">暂无活跃记忆</div>`;
    }
    return html`
      <div class="memory-list">
        ${this.memories.map(
          (m) => html`
            <div class="card memory-card ${m.pinned ? "pinned" : ""}">
              <div class="memory-card__header">
                <span class="chip chip--zone-${m.zone}">${ZONE_LABELS[m.zone] || m.zone}</span>
                <span class="chip">${SOURCE_LABELS[m.source] || m.source}</span>
                <span class="memory-card__meta">${CONFIDENCE_LABELS[m.confidence] || m.confidence}</span>
                <span class="memory-card__meta">${(m.effectiveness * 100).toFixed(0)}% 有效</span>
                ${m.pinned ? html`<span class="chip chip--pinned">已置顶</span>` : ""}
              </div>
              <div class="memory-card__body">${m.content}</div>
              <div class="memory-card__footer">
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
      return html`<div class="memory-empty">暂无待审核项</div>`;
    }
    return html`
      <div class="memory-list">
        ${this.queue.map((item) => {
          let data: any = {};
          try {
            data = JSON.parse(item.candidateData);
          } catch {
            // use raw data
          }
          return html`
            <div class="card memory-card">
              <div class="memory-card__header">
                <span class="chip chip--zone-${item.candidateType === "memory" ? "core" : "work"}">${item.candidateType === "memory" ? "记忆" : "技能"}</span>
                <span class="memory-card__meta">${item.createdAt?.slice(0, 16) || ""}</span>
              </div>
              <div class="memory-card__body">
                ${data.fact || data.name || data.description || item.candidateData}
              </div>
              ${data.reasoning
                ? html`<div class="queue-card__reasoning">理由: ${data.reasoning}</div>`
                : ""}
              <div class="memory-card__footer">
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

  @state() private profile: string | null = null;
  @state() private digest: string | null = null;

  private async handleCompileProfile() {
    this.loading = true;
    try {
      const res = await compileProfile(this.agentId);
      this.profile = res.result?.profile || null;
    } catch (e: any) {
      this.error = e.message || "编译失败";
    } finally {
      this.loading = false;
    }
  }

  private async handleGenerateDigest() {
    this.loading = true;
    try {
      const res = await generateWeeklyDigest(this.agentId);
      this.digest = res.result?.digest || null;
    } catch (e: any) {
      this.error = e.message || "生成失败";
    } finally {
      this.loading = false;
    }
  }

  private renderAudit() {
    return html`
      <div class="audit-panel">
        <div class="card">
          <h3 class="card-title">画像编译</h3>
          <p class="audit-section__desc">将活跃记忆编译为 Agent 画像 (PROFILE.md)</p>
          <button class="btn btn--primary" @click=${this.handleCompileProfile} ?disabled=${this.loading}>
            ${this.loading ? "编译中…" : "编译画像"}
          </button>
          ${this.profile ? html`<pre class="code-block audit-section__result">${this.profile}</pre>` : ""}
        </div>
        <div class="card">
          <h3 class="card-title">周报生成</h3>
          <p class="audit-section__desc">基于最近 7 天记忆生成学习周报</p>
          <button class="btn btn--primary" @click=${this.handleGenerateDigest} ?disabled=${this.loading}>
            ${this.loading ? "生成中…" : "生成周报"}
          </button>
          ${this.digest ? html`<pre class="code-block audit-section__result">${this.digest}</pre>` : ""}
        </div>
      </div>`;
  }
}
