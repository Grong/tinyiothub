import { LitElement, html, nothing } from "lit";
import { customElement, state } from "lit/decorators.js";
import { tagApi } from "../../api/tags.js";
import type { Tag } from "../../types/index.js";
import { success, error as toastError } from "../components/toast.js";

@customElement("view-tags")
export class TagsView extends LitElement {
  @state() loading = true;
  @state() error = "";
  @state() tags: Tag[] = [];
  @state() searchKeyword = "";

  @state() showModal = false;
  @state() editingTag: Tag | null = null;
  @state() saving = false;
  @state() formName = "";
  @state() formType = "";
  @state() formDescription = "";
  @state() formColor = "";

  createRenderRoot() {
    return this;
  }

  connectedCallback() {
    super.connectedCallback();
    this.loadData();
  }

  async loadData() {
    this.loading = true;
    this.error = "";
    try {
      const res = await tagApi.getTags();
      const data = res.result;
      if (data) {
        this.tags = data.data || [];
      }
    } catch (err: any) {
      this.error = err.message || "加载标签失败";
    } finally {
      this.loading = false;
    }
  }

  get filteredTags(): Tag[] {
    if (!this.searchKeyword) return this.tags;
    const kw = this.searchKeyword.toLowerCase();
    return this.tags.filter(t =>
      t.name.toLowerCase().includes(kw) ||
      t.type.toLowerCase().includes(kw) ||
      (t.description || "").toLowerCase().includes(kw)
    );
  }

  openCreate() {
    this.editingTag = null;
    this.formName = "";
    this.formType = "";
    this.formDescription = "";
    this.formColor = "#3b82f6";
    this.showModal = true;
  }

  openEdit(tag: Tag) {
    this.editingTag = tag;
    this.formName = tag.name;
    this.formType = tag.type;
    this.formDescription = tag.description || "";
    this.formColor = tag.color || "#3b82f6";
    this.showModal = true;
  }

  closeModal() {
    this.showModal = false;
    this.editingTag = null;
  }

  async saveForm() {
    if (!this.formName.trim() || !this.formType.trim()) return;
    this.saving = true;
    try {
      if (this.editingTag) {
        await tagApi.updateTag(this.editingTag.id, { name: this.formName });
        success("标签已更新");
      } else {
        await tagApi.createTag({
          name: this.formName,
          type: this.formType,
          description: this.formDescription || undefined,
          color: this.formColor || undefined,
        });
        success("标签已创建");
      }
      this.closeModal();
      await this.loadData();
    } catch (err: any) {
      toastError(err.message || "操作失败");
    } finally {
      this.saving = false;
    }
  }

  async deleteTag(tag: Tag) {
    if (!confirm(`确定要删除标签 "${tag.name}" 吗？`)) return;
    try {
      await tagApi.deleteTag(tag.id);
      success("标签已删除");
      await this.loadData();
    } catch (err: any) {
      toastError(err.message || "删除失败");
    }
  }

  render() {
    if (this.loading) {
      return html`
        <div style="display: flex; align-items: center; justify-content: center; padding: 60px;">
          <span class="loading-spinner"></span>
          <span style="margin-left: 8px; color: var(--muted);">加载中...</span>
        </div>
      `;
    }

    if (this.error) {
      return html`
        <div style="text-align: center; padding: 60px;">
          <div style="color: var(--danger); margin-bottom: 12px;">${this.error}</div>
          <button class="btn btn--primary" @click=${this.loadData}>重试</button>
        </div>
      `;
    }

    return html`
      <div style="display: flex; gap: 10px; margin-bottom: 16px; align-items: center; flex-wrap: wrap;">
        <div class="field" style="flex: 1; max-width: 280px; min-width: 160px;">
          <input
            type="text"
            placeholder="搜索标签名称、类型..."
            .value=${this.searchKeyword}
            @input=${(e: Event) => { this.searchKeyword = (e.target as HTMLInputElement).value; }}
          />
        </div>
        <button class="btn btn--primary" @click=${this.openCreate}>新建标签</button>
      </div>
      <div class="card" style="overflow: hidden;">
        <table style="width: 100%; border-collapse: collapse;">
          <thead>
            <tr style="border-bottom: 1px solid var(--border);">
              <th style="padding: 12px 16px; text-align: left; font-size: 13px; color: var(--muted); font-weight: 500;">标签名称</th>
              <th style="padding: 12px 16px; text-align: left; font-size: 13px; color: var(--muted); font-weight: 500;">类型</th>
              <th style="padding: 12px 16px; text-align: left; font-size: 13px; color: var(--muted); font-weight: 500;">颜色</th>
              <th style="padding: 12px 16px; text-align: left; font-size: 13px; color: var(--muted); font-weight: 500;">绑定数</th>
              <th style="padding: 12px 16px; text-align: left; font-size: 13px; color: var(--muted); font-weight: 500;">创建时间</th>
              <th style="padding: 12px 16px; text-align: right; font-size: 13px; color: var(--muted); font-weight: 500;">操作</th>
            </tr>
          </thead>
          <tbody>
            ${this.filteredTags.length === 0
              ? html`<tr><td colspan="6" style="padding: 40px; text-align: center; color: var(--muted);">暂无标签</td></tr>`
              : this.filteredTags.map(t => html`
                <tr style="border-bottom: 1px solid var(--border);">
                  <td style="padding: 12px 16px;">
                    <div style="font-weight: 500;">${t.name}</div>
                    ${t.description ? html`<div style="font-size: 12px; color: var(--muted);">${t.description}</div>` : nothing}
                  </td>
                  <td style="padding: 12px 16px; font-size: 13px;">${t.type}</td>
                  <td style="padding: 12px 16px;">
                    ${t.color ? html`
                      <span style="display: inline-block; width: 16px; height: 16px; border-radius: 4px; background: ${t.color};"></span>
                    ` : html`<span style="font-size: 13px; color: var(--muted);">-</span>`}
                  </td>
                  <td style="padding: 12px 16px; font-size: 13px;">${t.bindingCount ?? 0}</td>
                  <td style="padding: 12px 16px; font-size: 13px; color: var(--muted);">${t.createdAt?.slice(0, 16)}</td>
                  <td style="padding: 12px 16px; text-align: right;">
                    <button class="btn btn--ghost btn--sm" style="font-size: 12px;" @click=${() => this.openEdit(t)}>编辑</button>
                    <button class="btn btn--ghost btn--sm" style="font-size: 12px; color: var(--danger);" @click=${() => this.deleteTag(t)}>删除</button>
                  </td>
                </tr>
              `)}
          </tbody>
        </table>
      </div>
      ${this.showModal ? this.renderModal() : nothing}
    `;
  }

  renderModal() {
    return html`
      <div class="modal-overlay" role="dialog" aria-modal="true" aria-label=${this.editingTag ? "编辑标签" : "新建标签"} @click=${this.closeModal}>
        <div class="modal" @click=${(e: Event) => e.stopPropagation()}>
          <div class="modal-header">${this.editingTag ? "编辑标签" : "新建标签"}</div>
          <div class="modal-body">
            <div class="field">
              <span>名称</span>
              <input type="text" placeholder="标签名称" .value=${this.formName} @input=${(e: any) => { this.formName = e.target.value; }} />
            </div>
            <div class="field" style="margin-top: 12px;">
              <span>类型</span>
              <input type="text" placeholder="如: location, device, custom" .value=${this.formType} @input=${(e: any) => { this.formType = e.target.value; }} />
            </div>
            <div class="field" style="margin-top: 12px;">
              <span>描述</span>
              <input type="text" placeholder="可选描述" .value=${this.formDescription} @input=${(e: any) => { this.formDescription = e.target.value; }} />
            </div>
            <div class="field" style="margin-top: 12px;">
              <span>颜色</span>
              <div style="display: flex; align-items: center; gap: 8px;">
                <input type="color" .value=${this.formColor} @input=${(e: any) => { this.formColor = e.target.value; }} style="width: 40px; height: 32px; padding: 0; border: none; cursor: pointer;" />
                <input type="text" .value=${this.formColor} @input=${(e: any) => { this.formColor = e.target.value; }} style="flex: 1;" />
              </div>
            </div>
          </div>
          <div class="modal-footer">
            <button class="btn btn--ghost" @click=${this.closeModal}>取消</button>
            <button class="btn btn--primary" ?disabled=${this.saving || !this.formName.trim() || !this.formType.trim()} @click=${this.saveForm}>
              ${this.saving ? "保存中..." : "保存"}
            </button>
          </div>
        </div>
      </div>
    `;
  }
}
