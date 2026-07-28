// Extracted from things.ts (eng-review T14 god-file split).
// Render helpers take the host view instance; behavior unchanged.
import { html, nothing } from "lit";
import { icons } from "../icons.js";
import { CATEGORY_LABELS, CATEGORY_ICONS, getLocalizedText, isFieldRequired } from "./things.js";
import type { DriverConfigOption } from "../../types/index.js";
import type { DevicesView } from "./things.js";

export function renderWizard(host: DevicesView) {
    const isStep1 = host.wizardStep === "template";
    return html`
      <div class="wizard-overlay" role="dialog" aria-modal="true" aria-label="物创建向导" @click=${(e: Event) => { if ((e.target as HTMLElement).classList.contains('wizard-overlay')) host.closeWizard(); }} @keydown=${(e: KeyboardEvent) => host.handleModalKeydown(e, host.closeWizard)}>
        <div class="wizard-dialog">
          <!-- Header -->
          <div class="wizard-dialog__header">
            <button class="wizard-dialog__back" aria-label="返回" @click=${isStep1 ? host.closeWizard : host.wizardBack}>
              <span class="rotate-90">${icons.arrowDown}</span>
              <span>${isStep1 ? "返回物列表" : "返回模板选择"}</span>
            </button>
            <span class="wizard-dialog__title">${isStep1 ? "选择物模板" : "填写物信息"}</span>
            <button class="modal-close wizard-dialog__close" aria-label="关闭" @click=${host.closeWizard}>✕</button>
          </div>
          <!-- Body -->
          <div class="wizard-dialog__body">
            ${isStep1 ? host.renderWizardTemplateSelection() : host.renderWizardDeviceInfo()}
          </div>
          ${!isStep1 ? html`
            <div class="wizard-form-footer">
              <button class="btn btn--ghost" @click=${host.wizardBack}>上一步</button>
              <button class="btn btn--primary" ?disabled=${host.wizardSaving || !host.wizName.trim()} @click=${host.submitWizard}>
                ${host.wizardSaving ? "创建中..." : "创建物"}
              </button>
            </div>
          ` : nothing}
        </div>
      </div>
    `;
  }


export function renderWizardTemplateSelection(host: DevicesView) {
    const groups = host.wizardTemplatesByCategory;
    const categories = Object.keys(groups);

    return html`
      <!-- Search bar -->
      <div class="wizard-search">
        <span class="wizard-search__icon">
          ${icons.search}
        </span>
        <input
          type="text"
          class="wizard-search__input"
          placeholder="搜索物模板..."
          .value=${host.wizTemplateSearch}
          @input=${(e: Event) => { host.wizTemplateSearch = (e.target as HTMLInputElement).value; }}
        />
      </div>

      ${host.wizTemplateLoading ? html`
        <div class="wizard-loading">
          <span class="loading-spinner"></span>
          <span class="wizard-loading__text">加载中...</span>
        </div>
      ` : host.filteredWizardTemplates.length === 0 ? html`
        <div class="wizard-empty">
          <div class="wizard-empty__icon">📦</div>
          <div class="wizard-empty__title">没有找到匹配的模板</div>
          <div class="wizard-empty__hint">尝试调整搜索条件或浏览其他分类</div>
        </div>
      ` : html`
        ${categories.map(cat => html`
          <div class="wizard-category">
            <div class="wizard-category__header">
              <span class="wizard-category__title">${CATEGORY_LABELS[cat] || cat}</span>
              <span class="wizard-category__count">${groups[cat].length} 个模板</span>
            </div>
            <div class="wizard-template-grid">
              ${groups[cat].map(t => host.renderTemplateCard(t))}
            </div>
          </div>
        `)}
      `}
    `;
  }


export function renderWizardDeviceInfo(host: DevicesView) {
    const t = host.wizSelectedTemplate;
    if (!t) return nothing;
    const displayName = getLocalizedText(t.displayName, t.name);
    const hasError = (name: string) => Boolean(host.wizValidationErrors[name]);
    const getError = (name: string) => host.wizValidationErrors[name] || "";

    return html`
      <div class="wizard-split">
        <!-- Left panel: form -->
        <div class="wizard-split__form wizard-fields">
          <div class="wizard-form-header">
            <div class="wizard-form-header__title">填写物信息</div>
            <button class="btn btn--ghost btn--sm" @click=${host.wizardBack}>切换模板</button>
          </div>

          <!-- Template summary chip -->
          <div class="template-chip">
            <span class="template-chip__icon">${CATEGORY_ICONS[t.category] || CATEGORY_ICONS.others}</span>
            <div class="template-chip__title-wrap">
              <div class="template-chip__title">${displayName}</div>
              <div class="template-chip__meta">
                ${t.manufacturer ? html`<span>${t.manufacturer} · </span>` : nothing}
                <span>${t.deviceType || t.category}</span>
                ${t.version ? html` · v${t.version}` : nothing}
              </div>
            </div>
            ${t.isBuiltin ? html`<span class="template-chip__badge">内置</span>` : nothing}
          </div>

          <!-- Device name -->
          <div class="field ${hasError('deviceName') ? 'field--error' : ''}">
            <span>物名称 <span class="form-label-required">*</span></span>
            <input
              type="text"
              placeholder="请输入物名称"
              .value=${host.wizName}
              @input=${(e: any) => { host.wizName = e.target.value; }}
            />
            ${hasError("deviceName") ? html`<div class="form-error">${getError("deviceName")}</div>` : nothing}
          </div>

          <!-- Device description -->
          <div class="field">
            <span>物描述 <span class="inline-muted">(可选)</span></span>
            <textarea
              placeholder="请输入物描述"
              rows="2"
              .value=${host.wizDescription}
              @input=${(e: any) => { host.wizDescription = e.target.value; }}
            ></textarea>
          </div>

          <!-- Device address -->
          <div class="field ${hasError('deviceAddress') ? 'field--error' : ''}">
            <span>物地址 ${isFieldRequired(t.deviceInfo, "address")
              ? html`<span class="form-label-required">*</span>`
              : html`<span class="inline-muted">(可选)</span>`}</span>
            <input
              type="text"
              placeholder="请输入物IP地址或连接地址"
              .value=${host.wizAddress}
              @input=${(e: any) => { host.wizAddress = e.target.value; }}
            />
            ${hasError("deviceAddress") ? html`<div class="form-error">${getError("deviceAddress")}</div>` : nothing}
          </div>

          <!-- Device position -->
          <div class="field">
            <span>安装位置 <span class="inline-muted">(可选)</span></span>
            <input
              type="text"
              placeholder="请输入物安装位置"
              .value=${host.wizPosition}
              @input=${(e: any) => { host.wizPosition = e.target.value; }}
            />
          </div>

          <!-- Driver select -->
          <div class="field">
            <span>物驱动 <span class="inline-muted">(选择适合的驱动程序)</span></span>
            <select .value=${host.wizDriver} @change=${(e: Event) => host.onWizardDriverSelect((e.target as HTMLSelectElement).value)}>
              <option value="">请选择驱动</option>
              ${host.driverNames.map(name => html`<option value=${name}>${name}</option>`)}
            </select>
            ${t.driverName && host.wizDriver !== t.driverName ? html`
              <div class="form-hint">模板默认驱动: ${t.driverName}</div>
            ` : nothing}
          </div>

          <!-- Driver config -->
          ${host.wizDriver ? html`
            <div class="wizard-form-section">
              <div class="wizard-form-section__header">
                <span class="wizard-form-section__title">驱动配置</span>
                <span class="wizard-form-section__meta">(${host.wizDriver})</span>
              </div>
              ${host.wizConfigLoading ? html`
                <div class="wizard-loading wizard-loading--compact">
                  <span class="loading-spinner"></span>
                  <span class="wizard-loading__text">加载驱动配置参数...</span>
                </div>
              ` : host.wizConfigOptions.length > 0 ? html`
                ${host.wizConfigOptions.map(opt => host.renderWizardConfigField(opt))}
              ` : html`
                <div class="empty-hint--sm">
                  该驱动无需额外配置参数
                </div>
              `}
            </div>
          ` : nothing}

          ${host.wizUnassignedResources.length > 0 ? html`
            <div class="wizard-form-section">
              <div class="wizard-form-section__header">
                <span class="wizard-form-section__title">挂载资源</span>
                <span class="wizard-form-section__meta">(${host.wizUnassignedResources.length} 个未指派)</span>
              </div>
              ${host.wizUnassignedResources.map((r: any) => html`
                <label class="field field--row" style="display: flex; align-items: center; gap: var(--space-2); padding: var(--space-1) 0;">
                  <input type="checkbox"
                    .checked=${host.wizSelectedResourceIds.has(r.id)}
                    @change=${(e: Event) => {
                      const checked = (e.target as HTMLInputElement).checked;
                      const next = new Set(host.wizSelectedResourceIds);
                      if (checked) { next.add(r.id); } else { next.delete(r.id); }
                      host.wizSelectedResourceIds = next;
                    }}
                  />
                  <span>${r.name || r.filePath}</span>
                  <span class="inline-muted">${r.resourceType}</span>
                </label>
              `)}
            </div>
          ` : nothing}
        </div>

        <!-- Right panel: template overview -->
        <div class="wizard-split__overview">
          ${host.renderTemplateOverview(t)}
        </div>
      </div>
    `;
  }


export function renderWizardConfigField(host: DevicesView, opt: DriverConfigOption) {
    const value = host.wizDriverConfig[opt.name] ?? "";
    const hasError = Boolean(host.wizValidationErrors[`driverConfig.${opt.name}`]);
    const errorMsg = host.wizValidationErrors[`driverConfig.${opt.name}`] || "";
    const placeholder = opt.defaultValue ? `默认: ${opt.defaultValue}` : `请输入${opt.label}`;

    return html`
      <div class="field ${hasError ? 'field--error' : ''}">
        <span>
          ${opt.label}
          ${opt.required ? html`<span class="form-label-required">*</span>` : html`<span class="inline-muted">(可选)</span>`}
          ${opt.defaultValue ? html`<span class="inline-muted inline-muted--spaced">· 默认: ${opt.defaultValue}</span>` : nothing}
        </span>
        ${opt.optionType === "boolean" ? html`
          <select .value=${value || (opt.defaultValue === "true" ? "true" : "false")} @change=${(e: Event) => {
            host.wizDriverConfig = { ...host.wizDriverConfig, [opt.name]: (e.target as HTMLSelectElement).value };
          }}>
            <option value="">请选择</option>
            <option value="true">是</option>
            <option value="false">否</option>
          </select>
        ` : opt.optionType === "number" ? html`
          <input type="number" .value=${value} placeholder=${placeholder} @input=${(e: any) => {
            host.wizDriverConfig = { ...host.wizDriverConfig, [opt.name]: e.target.value };
          }} />
        ` : html`
          <input type="text" .value=${value} placeholder=${placeholder} @input=${(e: any) => {
            host.wizDriverConfig = { ...host.wizDriverConfig, [opt.name]: e.target.value };
          }} />
        `}
        ${hasError ? html`<div class="form-error">${errorMsg}</div>` : nothing}
      </div>
    `;
  }

