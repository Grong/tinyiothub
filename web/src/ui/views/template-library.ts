/**
 * E3 模板库（spec 2026-10-21-e3 §5）：场景包自带的 watch/composite 规则。
 * 选择模板 → 填引用映射 → 校验（逐条报告）→ 应用（整包事务）。
 */

import { LitElement, html, nothing } from 'lit';
import { customElement, property, state } from 'lit/decorators.js';
import './template-library.css';
import { sceneApi } from '../../api/marketplace.js';
import { templateRulesApi, type TemplateRulesReport } from '../../api/scene-pack.js';

type Localized = string | { zh?: string; en?: string } | undefined;

interface TemplateItem {
  id: string;
  name: string;
  displayName?: string;
  description?: string;
  isComposition?: boolean;
}

interface TemplateDetail extends TemplateItem {
  watchRules?: Array<Record<string, unknown>>;
  composites?: Array<Record<string, unknown>>;
}

function localized(value: Localized): string | undefined {
  if (typeof value === 'string') return value;
  return value?.zh ?? value?.en;
}

function collectRefs(template: { watch_rules?: unknown[]; composites?: unknown[] }): string[] {
  const refs = new Set<string>();
  const push = (v: unknown) => {
    if (typeof v === 'string' && v) refs.add(v);
  };
  for (const rule of template.watch_rules ?? []) {
    push((rule as Record<string, unknown>).thing);
  }
  for (const composite of template.composites ?? []) {
    const steps = ((composite as Record<string, unknown>).steps ?? []) as Array<Record<string, unknown>>;
    for (const step of steps) {
      push(step.thing);
      const rollback = step.rollback as Record<string, unknown> | undefined;
      if (rollback) push(rollback.thing);
    }
  }
  return [...refs];
}

@customElement('template-library')
export class TemplateLibrary extends LitElement {
  @property({ type: String }) workspaceId = '';
  @state() private loading = true;
  @state() private error = '';
  @state() private templates: TemplateItem[] = [];
  @state() private selected: TemplateDetail | null = null;
  @state() private mapping: Record<string, string> = {};
  @state() private report: TemplateRulesReport | null = null;
  @state() private busy = false;
  @state() private applied = false;

  connectedCallback(): void {
    super.connectedCallback();
    void this.load();
  }

  private async load(): Promise<void> {
    this.loading = true;
    this.error = '';
    try {
      const result = await sceneApi.listThingTemplates(true);
      this.templates = (result?.data ?? []).map((t) => ({
        id: t.id,
        name: t.name,
        displayName: localized(t.displayName as Localized),
        description: localized(t.description as Localized),
        isComposition: t.isComposition,
      }));
      this.loading = false;
    } catch (e) {
      this.loading = false;
      this.error = e instanceof Error ? e.message : String(e);
    }
  }

  private async pick(id: string): Promise<void> {
    this.loading = true;
    this.report = null;
    this.applied = false;
    try {
      const detail = (await sceneApi.getThingTemplate(id)) as TemplateDetail;
      this.selected = detail;
      this.mapping = Object.fromEntries(collectRefs(detail).map((r) => [r, '']));
      this.loading = false;
    } catch (e) {
      this.loading = false;
      this.error = e instanceof Error ? e.message : String(e);
    }
  }

  private payload(): { watch_rules: unknown[]; composites: unknown[] } | null {
    if (!this.selected) return null;
    return {
      watch_rules: this.selected.watchRules ?? [],
      composites: this.selected.composites ?? [],
    };
  }

  private async validate(): Promise<void> {
    const template = this.payload();
    if (!template) return;
    this.busy = true;
    try {
      this.report = await templateRulesApi.validate(template, this.mapping);
    } catch (e) {
      this.error = e instanceof Error ? e.message : String(e);
    } finally {
      this.busy = false;
    }
  }

  private async apply(): Promise<void> {
    const template = this.payload();
    if (!template) return;
    this.busy = true;
    try {
      const result = await templateRulesApi.apply(template, this.mapping);
      this.report = result.report;
      this.applied = result.applied;
      if (result.applied) {
        this.dispatchEvent(new CustomEvent('rules-applied', { bubbles: true, composed: true }));
      }
    } catch (e) {
      this.error = e instanceof Error ? e.message : String(e);
    } finally {
      this.busy = false;
    }
  }

  private close(): void {
    this.dispatchEvent(new CustomEvent('templates-closed', { bubbles: true, composed: true }));
  }

  private renderReport(): unknown {
    const report = this.report;
    if (!report) return nothing;
    const hasRules = (this.selected?.watchRules?.length ?? 0) + (this.selected?.composites?.length ?? 0) > 0;
    return html`
      <section class="tl-report ${report.ok ? 'tl-report--ok' : 'tl-report--bad'}" role="status">
        <div class="tl-report__head">
          ${report.ok ? '校验通过' : '校验未通过'} · watch ${report.resolved.watchRules} 条 · composite ${report.resolved.composites} 条
          ${hasRules ? nothing : html`<span class="tl-report__none">该模板未携带值守规则</span>`}
        </div>
        ${report.references.length
          ? html`<ul class="tl-report__list">
              ${report.references.map(
                (r) => html`<li><code>${r.path}</code> · ${r.refId ?? ''} · ${r.status}：${r.message}</li>`,
              )}
            </ul>`
          : nothing}
        ${report.conflicts.length
          ? html`<ul class="tl-report__list">
              ${report.conflicts.map((c) => html`<li>冲突 · ${c.refId}：${c.message}</li>`)}
            </ul>`
          : nothing}
        ${this.applied
          ? html`<div class="tl-report__applied">已应用：规则与协同链已写入本工作区。</div>`
          : nothing}
      </section>
    `;
  }

  render() {
    if (this.loading) {
      return html`<div class="tl-overlay"><div class="tl-panel tl-state">加载模板…</div></div>`;
    }
    if (this.error) {
      return html`
        <div class="tl-overlay" @click=${(e: Event) => { if (e.target === e.currentTarget) this.close(); }}>
          <div class="tl-panel">
            <header class="tl-header"><h3>模板库</h3><button class="btn btn--ghost btn--sm" @click=${() => this.close()}>关闭</button></header>
            <div class="tl-state tl-state--error">
              ${this.error}
              <button class="btn btn--primary btn--sm" @click=${() => this.load()}>重试</button>
            </div>
          </div>
        </div>
      `;
    }

    const refs = this.selected ? Object.keys(this.mapping) : [];
    return html`
      <div class="tl-overlay" @click=${(e: Event) => { if (e.target === e.currentTarget) this.close(); }}>
        <div class="tl-panel" role="dialog" aria-modal="true" aria-label="模板库">
          <header class="tl-header">
            <h3>模板库</h3>
            <button class="btn btn--ghost btn--sm" @click=${() => this.close()} aria-label="关闭">关闭</button>
          </header>

          <div class="tl-body">
            <ul class="tl-list">
              ${this.templates.length
                ? this.templates.map(
                    (t) => html`
                      <li>
                        <button
                          class="tl-card ${this.selected?.id === t.id ? 'is-active' : ''}"
                          @click=${() => this.pick(t.id)}
                        >
                          <span class="tl-card__name">${t.displayName || t.name}</span>
                          <span class="tl-card__meta">${t.isComposition ? '场景包' : '设备模板'}</span>
                        </button>
                      </li>
                    `,
                  )
                : html`<li class="tl-state tl-state--empty">还没有可用模板</li>`}
            </ul>

            <div class="tl-detail">
              ${this.selected
                ? html`
                    <h4>${this.selected.displayName || this.selected.name}</h4>
                    <p class="tl-detail__desc">${this.selected.description || ''}</p>
                    ${refs.length
                      ? html`
                          <div class="tl-mapping">
                            <div class="tl-mapping__title">引用映射（slug → 真实设备 ID）</div>
                            ${refs.map(
                              (ref) => html`
                                <label class="tl-mapping__row">
                                  <span>${ref}</span>
                                  <input
                                    class="input"
                                    .value=${this.mapping[ref]}
                                    @input=${(e: Event) => {
                                      this.mapping = {
                                        ...this.mapping,
                                        [ref]: (e.target as HTMLInputElement).value,
                                      };
                                    }}
                                  />
                                </label>
                              `,
                            )}
                          </div>
                        `
                      : html`<div class="tl-state tl-state--empty">该模板没有 watch/composite 规则引用。</div>`}
                    <div class="tl-actions">
                      <button class="btn btn--outline btn--sm" ?disabled=${this.busy} @click=${() => this.validate()}>校验</button>
                      <button
                        class="btn btn--primary btn--sm"
                        ?disabled=${this.busy || !this.report?.ok || refs.some((r) => !this.mapping[r])}
                        @click=${() => this.apply()}
                      >
                        应用
                      </button>
                    </div>
                    ${this.renderReport()}
                  `
                : html`<div class="tl-state tl-state--empty">选择一个模板查看规则。</div>`}
            </div>
          </div>
        </div>
      </div>
    `;
  }
}

declare global {
  interface HTMLElementTagNameMap {
    'template-library': TemplateLibrary;
  }
}
