import { LitElement, html, nothing } from "lit";
import { customElement, state } from "lit/decorators.js";
import { classMap } from "lit/directives/class-map.js";
import { cronApi } from "../../api/cron.js";
import { deviceApi } from "../../api/devices.js";
import type {
  Job,
  JobExecution,
  JobStatistics,
  CreateJobRequest,
} from "../../types/index.js";
import { success, error as toastError } from "../components/toast.js";

const JOB_TYPE_LABELS: Record<string, string> = {
  http: "HTTP 请求",
  script: "脚本执行",
  device_command: "设备命令",
  sql: "SQL 查询",
};

const JOB_TYPE_ICONS: Record<string, string> = {
  http: "M10 13a5 5 0 0 0 7.54.54l3-3a5 5 0 0 0-7.07-7.07l-1.72 1.71M14 11a5 5 0 0 0-7.54-.54l-3 3a5 5 0 0 0 7.07 7.07l1.71-1.71",
  script: "M14 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V8z M14 2v6h6 M16 13H8 M16 17H8 M10 9H8",
  device_command: "M12 2L2 7l10 5 10-5-10-5zM2 17l10 5 10-5M2 12l10 5 10-5",
  sql: "M4 6h16M4 12h16M4 18h16",
};

function formatDuration(ms?: number): string {
  if (ms == null || ms < 0) return "-";
  if (ms < 1000) return `${ms}ms`;
  if (ms < 60000) return `${(ms / 1000).toFixed(1)}s`;
  return `${(ms / 60000).toFixed(1)}m`;
}

function formatRelativeTime(dateStr?: string): string {
  if (!dateStr) return "-";
  const date = new Date(dateStr);
  const now = new Date();
  const diff = now.getTime() - date.getTime();
  const seconds = Math.floor(diff / 1000);
  if (seconds < 60) return `${seconds}秒前`;
  const minutes = Math.floor(seconds / 60);
  if (minutes < 60) return `${minutes}分钟前`;
  const hours = Math.floor(minutes / 60);
  if (hours < 24) return `${hours}小时前`;
  const days = Math.floor(hours / 24);
  if (days < 30) return `${days}天前`;
  return dateStr.slice(0, 16);
}

function formatNextRun(dateStr?: string): string {
  if (!dateStr) return "-";
  const date = new Date(dateStr);
  const now = new Date();
  const diff = date.getTime() - now.getTime();
  if (diff < 0) return "即将执行";
  const minutes = Math.floor(diff / 60000);
  if (minutes < 1) return "<1分钟";
  if (minutes < 60) return `${minutes}分钟后`;
  const hours = Math.floor(minutes / 60);
  if (hours < 24) return `${hours}小时后`;
  const days = Math.floor(hours / 24);
  return `${days}天后`;
}

function getDefaultConfig(jobType: string): string {
  switch (jobType) {
    case "http":
      return JSON.stringify(
        { url: "http://", method: "GET", headers: {}, body: {} },
        null,
        2
      );
    case "script":
      return JSON.stringify(
        { script: "echo 'hello'", interpreter: "bash", working_dir: "." },
        null,
        2
      );
    case "device_command":
      return JSON.stringify({ device_id: "", command_name: "" }, null, 2);
    case "sql":
      return JSON.stringify({ sql: "SELECT 1" }, null, 2);
    default:
      return "{}";
  }
}

function humanReadableCron(cron: string): string {
  const parts = cron.trim().split(/\s+/);
  if (parts.length !== 5) return "";
  const [min, hour, day, month, dow] = parts;

  // Common patterns
  if (cron === "*/5 * * * *") return "每5分钟";
  if (cron === "*/10 * * * *") return "每10分钟";
  if (cron === "*/15 * * * *") return "每15分钟";
  if (cron === "*/30 * * * *") return "每30分钟";
  if (cron === "0 * * * *") return "每小时";
  if (cron === "0 */2 * * *") return "每2小时";
  if (cron === "0 */6 * * *") return "每6小时";
  if (cron === "0 */12 * * *") return "每12小时";
  if (cron === "0 0 * * *") return "每天";
  if (cron === "0 0 * * 0") return "每周日";
  if (cron === "0 0 * * 1") return "每周一";
  if (cron === "0 0 1 * *") return "每月1日";

  // Partial patterns
  if (min.startsWith("*/") && hour === "*" && day === "*" && month === "*" && dow === "*") {
    return `每${min.slice(2)}分钟`;
  }
  if (min === "0" && hour.startsWith("*/") && day === "*" && month === "*" && dow === "*") {
    return `每${hour.slice(2)}小时`;
  }
  if (min === "0" && hour === "0" && day === "*" && month === "*") {
    if (dow === "*") return "每天";
    const weekdays = ["", "周一", "周二", "周三", "周四", "周五", "周六", "周日"];
    const idx = parseInt(dow, 10);
    if (weekdays[idx]) return `每周${weekdays[idx]}`;
  }
  if (min === "0" && hour === "0" && day === "1" && month === "*" && dow === "*") {
    return "每月1日";
  }

  return "";
}

@customElement("view-cron")
export class CronView extends LitElement {
  @state() loading = true;
  @state() error = "";
  @state() jobs: Job[] = [];
  @state() statistics: JobStatistics | null = null;
  @state() searchQuery = "";
  @state() filterType = "";
  @state() filterStatus = ""; // "enabled" | "disabled" | "running" | ""

  // Pagination
  @state() page = 1;
  @state() pageSize = 10;

  // Modal state
  @state() showModal = false;
  @state() editingJob: Job | null = null;
  @state() saving = false;

  // Form fields
  @state() formName = "";
  @state() formDescription = "";
  @state() formType = "http";
  @state() formCron = "*/5 * * * *";
  @state() formConfig = "";
  @state() formTimeout = "300";
  @state() formRetryCount = "0";
  @state() formRetryDelay = "60";
  @state() formConcurrency = "1";
  @state() formEnabled = true;
  @state() formTargetDevice = "";
  @state() formTargetCommand = "";
  @state() formConfigError = "";
  @state() formCronError = "";

  // Dirty state tracking (snapshot of initial form values when modal opened)
  private formSnapshot: Record<string, unknown> | null = null;

  // Executions
  @state() executionsJobId: string | null = null;
  @state() executions: JobExecution[] = [];
  @state() executionsLoading = false;
  @state() showExecutionsPanel = false;

  // Running state
  @state() runningJobId: string | null = null;

  // Devices list for device_command type
  @state() devices: Array<{ id: string; name: string }> = [];

  private modalLastFocus?: Element;
  private searchDebounceTimer?: ReturnType<typeof setTimeout>;

  createRenderRoot() {
    return this;
  }

  connectedCallback() {
    super.connectedCallback();
    this.loadData();
    this.loadDevices();
  }

  disconnectedCallback() {
    super.disconnectedCallback();
    if (this.searchDebounceTimer) {
      clearTimeout(this.searchDebounceTimer);
    }
    this.unlockScroll();
  }

  private lockScroll() {
    document.body.style.overflow = 'hidden';
  }

  private unlockScroll() {
    document.body.style.overflow = '';
  }

  async loadData() {
    this.loading = true;
    this.error = "";
    try {
      const [jobsRes, statsRes] = await Promise.all([
        cronApi.getJobs(),
        cronApi.getStatistics(),
      ]);
      this.jobs = jobsRes.result || [];
      this.statistics = statsRes.result || null;
    } catch (err: any) {
      this.error = err.message || "加载定时任务失败";
    } finally {
      this.loading = false;
    }
  }

  async loadDevices() {
    try {
      const res = await deviceApi.getDevices({ pageSize: 200 });
      const data = res.result;
      if (data?.data) {
        this.devices = data.data.map((d) => ({ id: d.id, name: d.displayName || d.name }));
      }
    } catch {
      // non-critical
    }
  }

  async loadExecutions(jobId: string) {
    this.executionsJobId = jobId;
    this.executionsLoading = true;
    this.showExecutionsPanel = true;
    try {
      const res = await cronApi.getJobExecutions(jobId, 20);
      this.executions = res.result || [];
    } catch (err: any) {
      toastError(err.message || "加载执行记录失败");
    } finally {
      this.executionsLoading = false;
    }
  }

  get filteredJobs(): Job[] {
    let result = this.jobs;
    if (this.searchQuery.trim()) {
      const q = this.searchQuery.trim().toLowerCase();
      result = result.filter(
        (j) =>
          j.name.toLowerCase().includes(q) ||
          (j.description && j.description.toLowerCase().includes(q)) ||
          j.cronExpression.includes(q)
      );
    }
    if (this.filterType) {
      result = result.filter((j) => j.jobType === this.filterType);
    }
    if (this.filterStatus) {
      switch (this.filterStatus) {
        case "enabled":
          result = result.filter((j) => j.isEnabled);
          break;
        case "disabled":
          result = result.filter((j) => !j.isEnabled);
          break;
        case "running":
          result = result.filter((j) => j.isRunning);
          break;
      }
    }
    const start = (this.page - 1) * this.pageSize;
    return result.slice(start, start + this.pageSize);
  }

  get totalFiltered(): number {
    let result = this.jobs;
    if (this.searchQuery.trim()) {
      const q = this.searchQuery.trim().toLowerCase();
      result = result.filter(
        (j) =>
          j.name.toLowerCase().includes(q) ||
          (j.description && j.description.toLowerCase().includes(q)) ||
          j.cronExpression.includes(q)
      );
    }
    if (this.filterType) {
      result = result.filter((j) => j.jobType === this.filterType);
    }
    if (this.filterStatus) {
      switch (this.filterStatus) {
        case "enabled":
          result = result.filter((j) => j.isEnabled);
          break;
        case "disabled":
          result = result.filter((j) => !j.isEnabled);
          break;
        case "running":
          result = result.filter((j) => j.isRunning);
          break;
      }
    }
    return result.length;
  }

  get totalPages(): number {
    return Math.max(1, Math.ceil(this.totalFiltered / this.pageSize));
  }

  get successRate(): number {
    const stats = this.statistics;
    if (!stats || stats.totalExecutions === 0) return 0;
    return Math.round((stats.successExecutions / stats.totalExecutions) * 100);
  }

  // === Modal ===

  openCreate() {
    this.modalLastFocus = document.activeElement ?? undefined;
    this.editingJob = null;
    this.formName = "";
    this.formDescription = "";
    this.formType = "http";
    this.formCron = "*/5 * * * *";
    this.formConfig = getDefaultConfig("http");
    this.formTimeout = "300";
    this.formRetryCount = "0";
    this.formRetryDelay = "60";
    this.formConcurrency = "1";
    this.formEnabled = true;
    this.formTargetDevice = "";
    this.formTargetCommand = "";
    this.formConfigError = "";
    this.formCronError = "";
    this.showModal = true;
    this.takeFormSnapshot();
    this.lockScroll();
    requestAnimationFrame(() => {
      const overlay = this.querySelector(".modal-overlay[role='dialog']");
      if (overlay) this.focusFirst(overlay as HTMLElement, 50);
    });
  }

  openEdit(job: Job) {
    this.modalLastFocus = document.activeElement ?? undefined;
    this.editingJob = job;
    this.formName = job.name;
    this.formDescription = job.description || "";
    this.formType = job.jobType;
    this.formCron = job.cronExpression;
    this.formConfig = job.config;
    this.formTimeout = String(job.timeoutSeconds ?? 300);
    this.formRetryCount = String(job.retryCount ?? 0);
    this.formRetryDelay = String(job.retryDelaySeconds ?? 60);
    this.formConcurrency = String(job.concurrency ?? 1);
    this.formEnabled = job.isEnabled;
    this.formTargetDevice = job.targetDeviceId || "";
    this.formTargetCommand = job.targetCommandName || "";
    this.formConfigError = "";
    this.formCronError = "";
    this.showModal = true;
    this.takeFormSnapshot();
    this.lockScroll();
    requestAnimationFrame(() => {
      const overlay = this.querySelector(".modal-overlay[role='dialog']");
      if (overlay) this.focusFirst(overlay as HTMLElement, 50);
    });
  }

  closeModal() {
    if (this.hasUnsavedChanges()) {
      if (!confirm("有未保存的更改，确定要放弃吗？")) return;
    }
    this.showModal = false;
    this.editingJob = null;
    this.formSnapshot = null;
    this.unlockScroll();
    const el = this.modalLastFocus as HTMLElement | undefined;
    if (el?.focus) {
      requestAnimationFrame(() => el.focus());
    }
    this.modalLastFocus = undefined;
  }

  private takeFormSnapshot() {
    this.formSnapshot = {
      name: this.formName,
      description: this.formDescription,
      type: this.formType,
      cron: this.formCron,
      config: this.formConfig,
      timeout: this.formTimeout,
      retryCount: this.formRetryCount,
      retryDelay: this.formRetryDelay,
      concurrency: this.formConcurrency,
      enabled: this.formEnabled,
      targetDevice: this.formTargetDevice,
      targetCommand: this.formTargetCommand,
    };
  }

  private hasUnsavedChanges(): boolean {
    if (!this.formSnapshot) return false;
    const s = this.formSnapshot;
    return (
      this.formName !== s.name ||
      this.formDescription !== s.description ||
      this.formType !== s.type ||
      this.formCron !== s.cron ||
      this.formConfig !== s.config ||
      this.formTimeout !== s.timeout ||
      this.formRetryCount !== s.retryCount ||
      this.formRetryDelay !== s.retryDelay ||
      this.formConcurrency !== s.concurrency ||
      this.formEnabled !== s.enabled ||
      this.formTargetDevice !== s.targetDevice ||
      this.formTargetCommand !== s.targetCommand
    );
  }

  onTypeChange(type: string) {
    this.formType = type;
    this.formConfig = getDefaultConfig(type);
  }

  validateConfig(): boolean {
    try {
      JSON.parse(this.formConfig);
      this.formConfigError = "";
      return true;
    } catch {
      this.formConfigError = "配置必须是有效的 JSON";
      return false;
    }
  }

  validateCron(): boolean {
    const cron = this.formCron.trim();
    if (!cron) {
      this.formCronError = "Cron 表达式不能为空";
      return false;
    }
    // Basic 5-field cron validation: field1 field2 field3 field4 field5
    const parts = cron.split(/\s+/);
    if (parts.length !== 5) {
      this.formCronError = "Cron 表达式必须是 5 个字段（分 时 日 月 周）";
      return false;
    }
    this.formCronError = "";
    return true;
  }

  async saveForm() {
    if (!this.formName.trim()) {
      toastError("任务名称不能为空");
      return;
    }
    if (!this.validateCron()) {
      toastError(this.formCronError);
      return;
    }
    if (!this.validateConfig()) {
      toastError("配置 JSON 格式错误");
      return;
    }
    this.saving = true;
    try {
      const payload: CreateJobRequest = {
        name: this.formName.trim(),
        description: this.formDescription.trim() || undefined,
        jobType: this.formType,
        cronExpression: this.formCron.trim(),
        config: this.formConfig,
        timeoutSeconds: parseInt(this.formTimeout, 10) || 300,
        retryCount: parseInt(this.formRetryCount, 10) || 0,
        retryDelaySeconds: parseInt(this.formRetryDelay, 10) || 60,
        concurrency: parseInt(this.formConcurrency, 10) || 1,
        targetDeviceId: this.formTargetDevice || undefined,
        targetCommandName: this.formTargetCommand || undefined,
      };
      if (this.editingJob) {
        await cronApi.updateJob(this.editingJob.id, payload);
        success("定时任务已更新");
      } else {
        await cronApi.createJob(payload);
        success("定时任务已创建");
      }
      this.closeModal();
      await this.loadData();
    } catch (err: any) {
      toastError(err.message || "保存失败");
    } finally {
      this.saving = false;
    }
  }

  async toggleJob(job: Job) {
    try {
      await cronApi.updateJob(job.id, { isEnabled: !job.isEnabled });
      success(job.isEnabled ? "任务已禁用" : "任务已启用");
      await this.loadData();
    } catch (err: any) {
      toastError(err.message || "操作失败");
    }
  }

  async runJob(job: Job) {
    if (this.runningJobId === job.id) return;
    this.runningJobId = job.id;
    try {
      await cronApi.runJobNow(job.id);
      success("任务已触发执行");
      await this.loadData();
      if (this.executionsJobId === job.id) {
        await this.loadExecutions(job.id);
      }
    } catch (err: any) {
      toastError(err.message || "触发执行失败");
    } finally {
      this.runningJobId = null;
    }
  }

  async deleteJob(job: Job) {
    if (!confirm(`确定要删除定时任务 "${job.name}" 吗？`)) return;
    try {
      await cronApi.deleteJob(job.id);
      success("任务已删除");
      if (this.executionsJobId === job.id) {
        this.showExecutionsPanel = false;
        this.executionsJobId = null;
        this.executions = [];
      }
      await this.loadData();
    } catch (err: any) {
      toastError(err.message || "删除失败");
    }
  }

  // === Focus management ===

  private focusFirst(container: HTMLElement, delay = 0) {
    setTimeout(() => {
      const el = container.querySelector<HTMLElement>(
        'button, [href], input, select, textarea, [tabindex]:not([tabindex="-1"])'
      );
      el?.focus();
    }, delay);
  }

  private handleModalKeydown(e: KeyboardEvent, closeFn: () => void) {
    if (e.key === "Escape") {
      e.preventDefault();
      closeFn();
      return;
    }
    if (e.key !== "Tab") return;
    const container = e.currentTarget as HTMLElement;
    if (!container) return;
    const focusables = Array.from(
      container.querySelectorAll<HTMLElement>(
        'a[href], button, textarea, input:not([type="hidden"]), select, [tabindex]:not([tabindex="-1"])'
      )
    ).filter((el) => !el.hasAttribute("disabled") && (el as HTMLElement).offsetParent !== null);
    if (focusables.length === 0) return;
    const first = focusables[0];
    const last = focusables[focusables.length - 1];
    if (e.shiftKey) {
      if (document.activeElement === first) {
        e.preventDefault();
        last.focus();
      }
    } else {
      if (document.activeElement === last) {
        e.preventDefault();
        first.focus();
      }
    }
  }

  // === Render ===

  render() {
    if (this.loading) {
      return this.renderSkeletons();
    }

    if (this.error) {
      return html`
        <div class="page-error" role="alert" aria-live="assertive">
          <div class="page-error__message">${this.error}</div>
          <button class="btn btn--primary" @click=${this.loadData}>重试</button>
        </div>
      `;
    }

    return html`
      ${this.renderStats()}
      <div class="cron-layout">
        <div class="cron-main">
          ${this.renderToolbar()}
          ${this.renderJobList()}
          ${this.renderJobCards()}
        </div>
        ${this.showExecutionsPanel ? this.renderExecutionsPanel() : nothing}
      </div>
      ${this.showModal ? this.renderModal() : nothing}
    `;
  }

  renderSkeletons() {
    return html`
      <div class="cron-stats">
        ${[1, 2, 3, 4].map(() => html`
          <div class="cron-stat-card skeleton-card">
            <div class="skeleton-circle"></div>
            <div class="skeleton-text-group">
              <div class="skeleton-line skeleton-line--short"></div>
              <div class="skeleton-line skeleton-line--long"></div>
            </div>
          </div>
        `)}
      </div>
      <div class="cron-layout">
        <div class="cron-main">
          <div class="toolbar cron-toolbar skeleton-toolbar">
            <div class="skeleton-line skeleton-line--search"></div>
            <div class="skeleton-line skeleton-line--select"></div>
            <div class="skeleton-line skeleton-line--select"></div>
            <div class="toolbar__spacer"></div>
            <div class="skeleton-line skeleton-line--btn"></div>
          </div>
          <div class="card skeleton-table">
            ${[1, 2, 3, 4, 5].map(() => html`
              <div class="skeleton-row">
                <div class="skeleton-line skeleton-line--cell-wide"></div>
                <div class="skeleton-line skeleton-line--cell"></div>
                <div class="skeleton-line skeleton-line--cell"></div>
                <div class="skeleton-line skeleton-line--cell"></div>
                <div class="skeleton-line skeleton-line--cell-actions"></div>
              </div>
            `)}
          </div>
        </div>
      </div>
    `;
  }

  renderStats() {
    const stats = this.statistics;
    return html`
      <div class="cron-stats">
        <div class="cron-stat-card">
          <div class="cron-stat-icon cron-stat-icon--total">
            <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
              <rect x="3" y="4" width="18" height="18" rx="2" ry="2"></rect>
              <line x1="16" y1="2" x2="16" y2="6"></line>
              <line x1="8" y1="2" x2="8" y2="6"></line>
              <line x1="3" y1="10" x2="21" y2="10"></line>
            </svg>
          </div>
          <div class="cron-stat-info">
            <div class="cron-stat-value">${stats?.totalJobs ?? 0}</div>
            <div class="cron-stat-label">总任务</div>
          </div>
        </div>
        <div class="cron-stat-card">
          <div class="cron-stat-icon cron-stat-icon--enabled">
            <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
              <path d="M22 11.08V12a10 10 0 1 1-5.93-9.14"></path>
              <polyline points="22 4 12 14.01 9 11.01"></polyline>
            </svg>
          </div>
          <div class="cron-stat-info">
            <div class="cron-stat-value">${stats?.enabledJobs ?? 0}</div>
            <div class="cron-stat-label">已启用</div>
          </div>
        </div>
        <div class="cron-stat-card">
          <div class="cron-stat-icon cron-stat-icon--running">
            <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
              <polygon points="13 2 3 14 12 14 11 22 21 10 12 10 13 2"></polygon>
            </svg>
          </div>
          <div class="cron-stat-info">
            <div class="cron-stat-value">${stats?.runningJobs ?? 0}</div>
            <div class="cron-stat-label">运行中</div>
          </div>
        </div>
        <div class="cron-stat-card">
          <div class="cron-stat-icon cron-stat-icon--success">
            <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
              <path d="M22 11.08V12a10 10 0 1 1-5.93-9.14"></path>
              <polyline points="22 4 12 14.01 9 11.01"></polyline>
            </svg>
          </div>
          <div class="cron-stat-info">
            <div class="cron-stat-value" aria-live="polite">${this.successRate}%</div>
            <div class="cron-stat-label">成功率</div>
          </div>
        </div>
      </div>
    `;
  }

  private onSearchInput(e: Event) {
    const value = (e.target as HTMLInputElement).value;
    if (this.searchDebounceTimer) {
      clearTimeout(this.searchDebounceTimer);
    }
    this.searchDebounceTimer = setTimeout(() => {
      this.searchQuery = value;
      this.page = 1;
      this.searchDebounceTimer = undefined;
    }, 250);
  }

  renderToolbar() {
    return html`
      <div class="toolbar cron-toolbar">
        <div class="field cron-toolbar__search">
          <input
            type="text"
            placeholder="搜索任务名称或 Cron 表达式..."
            .value=${this.searchQuery}
            @input=${this.onSearchInput}
          />
        </div>
        <select class="select" .value=${this.filterType} @change=${(e: Event) => {
          this.filterType = (e.target as HTMLSelectElement).value;
          this.page = 1;
        }}>
          <option value="">全部类型</option>
          <option value="http">HTTP 请求</option>
          <option value="script">脚本执行</option>
          <option value="device_command">设备命令</option>
          <option value="sql">SQL 查询</option>
        </select>
        <select class="select" .value=${this.filterStatus} @change=${(e: Event) => {
          this.filterStatus = (e.target as HTMLSelectElement).value;
          this.page = 1;
        }}>
          <option value="">全部状态</option>
          <option value="enabled">已启用</option>
          <option value="disabled">已禁用</option>
          <option value="running">运行中</option>
        </select>
        ${this.searchQuery || this.filterType || this.filterStatus
          ? html`
              <button
                class="btn btn--ghost btn--sm"
                @click=${() => {
                  this.searchQuery = "";
                  this.filterType = "";
                  this.filterStatus = "";
                  this.page = 1;
                }}
                aria-label="清除筛选"
              >
                清除筛选
              </button>
            `
          : nothing}
        <div class="toolbar__spacer"></div>
        <button class="btn btn--primary" @click=${this.openCreate}>新建任务</button>
      </div>
    `;
  }

  renderJobList() {
    const jobs = this.filteredJobs;
    const total = this.totalFiltered;
    const totalPages = this.totalPages;
    const start = total === 0 ? 0 : (this.page - 1) * this.pageSize + 1;
    const end = Math.min(this.page * this.pageSize, total);

    return html`
      <div class="card table-container">
        <table class="data-table">
          <thead>
            <tr>
              <th scope="col">任务名称</th>
              <th scope="col">类型</th>
              <th scope="col">Cron 表达式</th>
              <th scope="col">状态</th>
              <th scope="col">下次运行</th>
              <th scope="col">最后执行</th>
              <th scope="col">执行次数</th>
              <th scope="col" class="cell-actions">操作</th>
            </tr>
          </thead>
          <tbody>
            ${jobs.length === 0
              ? html`
                  <tr>
                    <td colspan="8" class="empty-hint">
                      <div class="cron-empty-state">
                        <span>暂无定时任务</span>
                        <button class="btn btn--primary btn--sm" @click=${this.openCreate}>新建任务</button>
                      </div>
                    </td>
                  </tr>
                `
              : jobs.map((job) => this.renderJobRow(job))}
          </tbody>
        </table>
        ${totalPages > 1
          ? html`
              <div class="table-pagination">
                <span class="table-pagination__info">${start}-${end} / ${total} 条</span>
                <div class="table-pagination__controls">
                  <button
                    class="btn btn--ghost btn--sm"
                    ?disabled=${this.page <= 1}
                    @click=${() => this.page = Math.max(1, this.page - 1)}
                    aria-label="上一页"
                  >
                    上一页
                  </button>
                  <span class="table-pagination__page">${this.page} / ${totalPages}</span>
                  <button
                    class="btn btn--ghost btn--sm"
                    ?disabled=${this.page >= totalPages}
                    @click=${() => this.page = Math.min(totalPages, this.page + 1)}
                    aria-label="下一页"
                  >
                    下一页
                  </button>
                </div>
              </div>
            `
          : nothing}
      </div>
    `;
  }

  renderJobRow(job: Job) {
    const typeLabel = JOB_TYPE_LABELS[job.jobType] || job.jobType;
    const isRunning = job.isRunning;
    const lastStatus = job.lastRunStatus;
    const statusClass = lastStatus === "success" ? "ok" : lastStatus === "failed" ? "error" : "";
    const isSelected = this.executionsJobId === job.id;

    return html`
      <tr class="${classMap({ "row-selected": isSelected })}">
        <td>
          <div class="data-table__primary">${job.name}</div>
          ${job.description ? html`<div class="data-table__secondary">${job.description}</div>` : nothing}
        </td>
        <td>
          <span class="cron-type-badge ${classMap({ [`cron-type-badge--${job.jobType}`]: true })}">
            <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" width="14" height="14">
              <path d="${JOB_TYPE_ICONS[job.jobType] || ""}"></path>
            </svg>
            ${typeLabel}
          </span>
        </td>
        <td>
          <code class="cron-expr">${job.cronExpression}</code>
          ${humanReadableCron(job.cronExpression)
            ? html`<div class="cron-human cron-hint">${humanReadableCron(job.cronExpression)}</div>`
            : nothing}
        </td>
        <td>
          ${isRunning
            ? html`<span class="status-badge status-badge--running">运行中</span>`
            : html`
                <span class="status-badge ${job.isEnabled ? "status-badge--online" : "status-badge--offline"}">
                  ${job.isEnabled ? "已启用" : "已禁用"}
                </span>
              `}
        </td>
        <td class="muted">${formatNextRun(job.nextRunAt)}</td>
        <td>
          ${lastStatus
            ? html`
                <span class="cron-last-status cron-last-status--${statusClass}">
                  ${lastStatus === "success" ? "成功" : lastStatus === "failed" ? "失败" : lastStatus}
                </span>
                <div class="muted cron-hint">
                  ${formatRelativeTime(job.lastRunAt)}
                </div>
              `
            : html`<span class="muted">-</span>`}
        </td>
        <td>
          <div class="cron-run-count">
            <span>${job.runCount}</span>
            ${job.successCount > 0 || job.failCount > 0
              ? html`
                  <span class="cron-run-count__detail">
                    <span class="ok">${job.successCount}</span> /
                    <span class="error">${job.failCount}</span>
                  </span>
                `
              : nothing}
          </div>
        </td>
        <td class="cell-actions">
          <button
            class="btn btn--ghost btn--sm"
            ?disabled=${this.runningJobId === job.id}
            @click=${() => this.runJob(job)}
            title="立即执行"
            aria-label="立即执行"
          >
            ${this.runningJobId === job.id ? "执行中..." : "运行"}
          </button>
          <button class="btn btn--ghost btn--sm" @click=${() => this.loadExecutions(job.id)} title="执行记录" aria-label="执行记录" aria-expanded="${this.executionsJobId === job.id && this.showExecutionsPanel}">
            记录
          </button>
          <button class="btn btn--ghost btn--sm" @click=${() => this.openEdit(job)} title="编辑" aria-label="编辑">
            编辑
          </button>
          <button
            class="btn btn--ghost btn--sm"
            @click=${() => this.toggleJob(job)}
            title="${job.isEnabled ? "禁用" : "启用"}"
            aria-label="${job.isEnabled ? "禁用" : "启用"}"
          >
            ${job.isEnabled ? "禁用" : "启用"}
          </button>
          <button class="btn btn--ghost btn--sm btn--danger-text" @click=${() => this.deleteJob(job)} title="删除" aria-label="删除">
            删除
          </button>
        </td>
      </tr>
    `;
  }

  renderJobCards() {
    const jobs = this.filteredJobs;
    const total = this.totalFiltered;
    const totalPages = this.totalPages;
    const start = total === 0 ? 0 : (this.page - 1) * this.pageSize + 1;
    const end = Math.min(this.page * this.pageSize, total);

    return html`
      <div class="cron-mobile-cards">
        ${jobs.length === 0
          ? html`
              <div class="cron-mobile-card empty-hint">
                <div class="cron-empty-state">
                  <span>暂无定时任务</span>
                  <button class="btn btn--primary btn--sm" @click=${this.openCreate}>新建任务</button>
                </div>
              </div>
            `
          : jobs.map((job) => this.renderJobCard(job))}
        ${totalPages > 1
          ? html`
              <div class="table-pagination">
                <span class="table-pagination__info">${start}-${end} / ${total} 条</span>
                <div class="table-pagination__controls">
                  <button
                    class="btn btn--ghost btn--sm"
                    ?disabled=${this.page <= 1}
                    @click=${() => this.page = Math.max(1, this.page - 1)}
                    aria-label="上一页"
                  >
                    上一页
                  </button>
                  <span class="table-pagination__page">${this.page} / ${totalPages}</span>
                  <button
                    class="btn btn--ghost btn--sm"
                    ?disabled=${this.page >= totalPages}
                    @click=${() => this.page = Math.min(totalPages, this.page + 1)}
                    aria-label="下一页"
                  >
                    下一页
                  </button>
                </div>
              </div>
            `
          : nothing}
      </div>
    `;
  }

  renderJobCard(job: Job) {
    const typeLabel = JOB_TYPE_LABELS[job.jobType] || job.jobType;
    const isRunning = job.isRunning;
    const lastStatus = job.lastRunStatus;
    const statusClass = lastStatus === "success" ? "ok" : lastStatus === "failed" ? "error" : "";
    const isSelected = this.executionsJobId === job.id;
    const humanCron = humanReadableCron(job.cronExpression);

    return html`
      <div class="cron-mobile-card ${classMap({ "cron-mobile-card--selected": isSelected })}">
        <div class="cron-mobile-card__header">
          <div class="cron-mobile-card__title">${job.name}</div>
          <span class="cron-type-badge ${classMap({ [`cron-type-badge--${job.jobType}`]: true })}">
            <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" width="12" height="12">
              <path d="${JOB_TYPE_ICONS[job.jobType] || ""}"></path>
            </svg>
            ${typeLabel}
          </span>
        </div>
        ${job.description ? html`<div class="cron-mobile-card__desc">${job.description}</div>` : nothing}
        <div class="cron-mobile-card__meta">
          <div class="cron-mobile-card__meta-item">
            <span class="cron-mobile-card__label">Cron</span>
            <code class="cron-expr">${job.cronExpression}</code>
            ${humanCron ? html`<span class="cron-human">${humanCron}</span>` : nothing}
          </div>
          <div class="cron-mobile-card__meta-item">
            <span class="cron-mobile-card__label">状态</span>
            ${isRunning
              ? html`<span class="status-badge status-badge--running">运行中</span>`
              : html`<span class="status-badge ${job.isEnabled ? "status-badge--online" : "status-badge--offline"}">
                      ${job.isEnabled ? "已启用" : "已禁用"}
                    </span>`}
          </div>
          <div class="cron-mobile-card__meta-item">
            <span class="cron-mobile-card__label">下次</span>
            <span class="muted">${formatNextRun(job.nextRunAt)}</span>
          </div>
          ${lastStatus
            ? html`
                <div class="cron-mobile-card__meta-item">
                  <span class="cron-mobile-card__label">最后</span>
                  <span class="cron-last-status cron-last-status--${statusClass}">
                    ${lastStatus === "success" ? "成功" : lastStatus === "failed" ? "失败" : lastStatus}
                  </span>
                  <span class="muted cron-hint">${formatRelativeTime(job.lastRunAt)}</span>
                </div>
              `
            : nothing}
          <div class="cron-mobile-card__meta-item">
            <span class="cron-mobile-card__label">次数</span>
            <span>${job.runCount}</span>
            ${job.successCount > 0 || job.failCount > 0
              ? html`<span class="cron-run-count__detail"><span class="ok">${job.successCount}</span> / <span class="error">${job.failCount}</span></span>`
              : nothing}
          </div>
        </div>
        <div class="cron-mobile-card__actions">
          <button
            class="btn btn--ghost btn--sm"
            ?disabled=${this.runningJobId === job.id}
            @click=${() => this.runJob(job)}
            title="立即执行"
            aria-label="立即执行"
          >
            ${this.runningJobId === job.id ? "执行中..." : "运行"}
          </button>
          <button class="btn btn--ghost btn--sm" @click=${() => this.loadExecutions(job.id)} title="执行记录" aria-label="执行记录" aria-expanded="${this.executionsJobId === job.id && this.showExecutionsPanel}">记录</button>
          <button class="btn btn--ghost btn--sm" @click=${() => this.openEdit(job)} title="编辑" aria-label="编辑">编辑</button>
          <button
            class="btn btn--ghost btn--sm"
            @click=${() => this.toggleJob(job)}
            title="${job.isEnabled ? "禁用" : "启用"}"
            aria-label="${job.isEnabled ? "禁用" : "启用"}"
          >
            ${job.isEnabled ? "禁用" : "启用"}
          </button>
          <button class="btn btn--ghost btn--sm btn--danger-text" @click=${() => this.deleteJob(job)} title="删除" aria-label="删除">删除</button>
        </div>
      </div>
    `;
  }

  renderExecutionsPanel() {
    const job = this.jobs.find((j) => j.id === this.executionsJobId);
    return html`
      <div class="card cron-executions">
        <div class="cron-executions__header">
          <div>
            <div class="card-title">执行记录</div>
            <div class="card-sub">${job ? job.name : ""}</div>
          </div>
          <div class="cron-flex-gap">
            <button
              class="btn btn--ghost btn--sm"
              ?disabled=${this.executionsLoading}
              @click=${() => this.executionsJobId && this.loadExecutions(this.executionsJobId)}
              aria-label="刷新执行记录"
              title="刷新"
            >
              <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" width="16" height="16">
                <polyline points="23 4 23 10 17 10"></polyline>
                <polyline points="1 20 1 14 7 14"></polyline>
                <path d="M3.51 9a9 9 0 0 1 14.85-3.36L23 10M1 14l4.64 4.36A9 9 0 0 0 20.49 15"></path>
              </svg>
            </button>
            <button class="btn btn--ghost btn--sm" @click=${() => {
              this.showExecutionsPanel = false;
              this.executionsJobId = null;
            }} aria-label="关闭执行记录面板">
              关闭
            </button>
          </div>
        </div>
        ${this.executionsLoading
          ? html`<div class="empty-hint">加载中...</div>`
          : this.executions.length === 0
            ? html`<div class="empty-hint">暂无执行记录</div>`
            : html`
                <div class="cron-execution-list">
                  ${this.executions.map((exec) => this.renderExecutionItem(exec))}
                </div>
              `}
      </div>
    `;
  }

  renderExecutionItem(exec: JobExecution) {
    const isSuccess = exec.status === "success";
    return html`
      <div class="cron-execution-item">
        <div class="cron-execution-item__main">
          <div class="cron-execution-item__status">
            <span class="cron-status-dot ${classMap({ "dot-ok": isSuccess, "dot-error": !isSuccess })}"></span>
            <span class="cron-execution-item__trigger">${exec.triggerType}</span>
          </div>
          <div class="cron-execution-item__time">
            ${exec.startedAt?.slice(0, 19) || "-"}
          </div>
          ${exec.errorMessage
            ? html`<div class="cron-execution-item__error">${exec.errorMessage}</div>`
            : nothing}
        </div>
        <div class="cron-execution-item__meta">
          <span class="chip ${classMap({ "chip-ok": isSuccess, "chip-danger": !isSuccess })}">
            ${isSuccess ? "成功" : "失败"}
          </span>
          ${exec.durationMs != null
            ? html`<span class="muted">${formatDuration(exec.durationMs)}</span>`
            : nothing}
        </div>
      </div>
    `;
  }

  renderModal() {
    const isEditing = !!this.editingJob;
    return html`
      <div
        class="modal-overlay"
        role="dialog"
        aria-modal="true"
        aria-label="${isEditing ? "编辑定时任务" : "新建定时任务"}"
        @click=${this.closeModal}
        @keydown=${(e: KeyboardEvent) => this.handleModalKeydown(e, this.closeModal)}
      >
        <div class="modal modal--wide" @click=${(e: Event) => e.stopPropagation()}>
          <div class="modal-header">
            <span>${isEditing ? "编辑定时任务" : "新建定时任务"}</span>
            <button
              class="btn btn--ghost btn--sm"
              @click=${this.closeModal}
              aria-label="关闭"
              title="关闭"
              class="cron-ml-auto"
            >
              <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" width="18" height="18">
                <line x1="18" y1="6" x2="6" y2="18"></line>
                <line x1="6" y1="6" x2="18" y2="18"></line>
              </svg>
            </button>
          </div>
          <div class="modal-body">
            <div class="form-grid cron-form-grid">
              <div class="field">
                <span>任务名称 <span class="cron-required">*</span></span>
                <input
                  type="text"
                  placeholder="输入任务名称"
                  .value=${this.formName}
                  @input=${(e: Event) => {
                    this.formName = (e.target as HTMLInputElement).value;
                  }}
                />
              </div>
              <div class="field">
                <span>任务类型</span>
                <select
                  .value=${this.formType}
                  ?disabled=${isEditing}
                  @change=${(e: Event) => {
                    this.onTypeChange((e.target as HTMLSelectElement).value);
                  }}
                >
                  <option value="http">HTTP 请求</option>
                  <option value="script">脚本执行</option>
                  <option value="device_command">设备命令</option>
                  <option value="sql">SQL 查询</option>
                </select>
              </div>
              <div class="field">
                <span>Cron 表达式 <span class="cron-required">*</span></span>
                <input
                  type="text"
                  placeholder="*/5 * * * *"
                  .value=${this.formCron}
                  @input=${(e: Event) => {
                    this.formCron = (e.target as HTMLInputElement).value;
                    this.formCronError = "";
                  }}
                />
                <div class="cron-help">例如: 每小时 0 * * * *，每天 0 0 * * *</div>
                ${this.formCronError
                  ? html`<div class="cron-help cron-error">${this.formCronError}</div>`
                  : nothing}
              </div>
              <div class="field">
                <span>描述</span>
                <input
                  type="text"
                  placeholder="可选描述"
                  .value=${this.formDescription}
                  @input=${(e: Event) => {
                    this.formDescription = (e.target as HTMLInputElement).value;
                  }}
                />
              </div>
              <div class="field">
                <span>超时时间（秒）</span>
                <input
                  type="number"
                  .value=${this.formTimeout}
                  @input=${(e: Event) => {
                    this.formTimeout = (e.target as HTMLInputElement).value;
                  }}
                />
              </div>
              <div class="field">
                <span>重试次数</span>
                <input
                  type="number"
                  .value=${this.formRetryCount}
                  @input=${(e: Event) => {
                    this.formRetryCount = (e.target as HTMLInputElement).value;
                  }}
                />
              </div>
              <div class="field">
                <span>重试间隔（秒）</span>
                <input
                  type="number"
                  .value=${this.formRetryDelay}
                  @input=${(e: Event) => {
                    this.formRetryDelay = (e.target as HTMLInputElement).value;
                  }}
                />
              </div>
              <div class="field">
                <span>并发数</span>
                <input
                  type="number"
                  .value=${this.formConcurrency}
                  @input=${(e: Event) => {
                    this.formConcurrency = (e.target as HTMLInputElement).value;
                  }}
                />
              </div>
              ${this.formType === "device_command"
                ? html`
                    <div class="field">
                      <span>目标设备</span>
                      <select
                        .value=${this.formTargetDevice}
                        @change=${(e: Event) => {
                          this.formTargetDevice = (e.target as HTMLSelectElement).value;
                        }}
                      >
                        <option value="">选择设备</option>
                        ${this.devices.map(
                          (d) => html`<option value=${d.id}>${d.name}</option>`
                        )}
                      </select>
                    </div>
                    <div class="field">
                      <span>命令名称</span>
                      <input
                        type="text"
                        placeholder="command_name"
                        .value=${this.formTargetCommand}
                        @input=${(e: Event) => {
                          this.formTargetCommand = (e.target as HTMLInputElement).value;
                        }}
                      />
                    </div>
                  `
                : nothing}
              <div class="field cron-span-2">
                <span>配置 JSON</span>
                <textarea
                  rows="6"
                  .value=${this.formConfig}
                  @input=${(e: Event) => {
                    this.formConfig = (e.target as HTMLTextAreaElement).value;
                    this.formConfigError = "";
                  }}
                ></textarea>
                ${this.formConfigError
                  ? html`<div class="cron-help cron-error">${this.formConfigError}</div>`
                  : nothing}
              </div>
              <label class="field checkbox cron-checkbox">
                <input
                  type="checkbox"
                  .checked=${this.formEnabled}
                  @change=${(e: Event) => {
                    this.formEnabled = (e.target as HTMLInputElement).checked;
                  }}
                />
                <span class="field-checkbox__label">启用任务</span>
              </label>
            </div>
          </div>
          <div class="modal-footer">
            <button class="btn btn--ghost" @click=${this.closeModal}>取消</button>
            <button
              class="btn btn--primary"
              ?disabled=${this.saving || !this.formName.trim()}
              @click=${this.saveForm}
            >
              ${this.saving ? "保存中..." : "保存"}
            </button>
          </div>
        </div>
      </div>
    `;
  }
}
