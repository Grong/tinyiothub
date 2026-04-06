# Full-Screen Device Creation Wizard — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace the modal-based device creation wizard with a full-screen, 2-step wizard. Step 1 is a full-screen template selection grid. Step 2 is a split-panel layout with the device form on the left and a template overview (stats, properties, commands) on the right.

**Architecture:** Pure Lit component render changes — no new files, no new state fields. Reuse existing `ProcessedTemplate`, helper methods, and state. Add CSS classes to `components.css`. Replace 4 render methods in `devices.ts` and add 1 new method.

**Tech Stack:** Lit 3 Web Components, vanilla CSS, existing design tokens (`--bg`, `--card`, `--border`, `--primary`, `--muted`, etc.)

---

## File Structure

Only 2 files are modified:

| File | Changes |
|------|---------|
| `web/src/styles/components.css` | Add `.wizard-fullscreen` CSS block (~80 lines) after `.modal-footer` (line 3628) |
| `web/src/ui/views/devices.ts` | Replace `renderWizard()`, `renderWizardTemplateSelection()`, `renderWizardDeviceInfo()`, `renderTemplateCard()`. Add new `renderTemplateOverview()` method. |

No new files. No new state fields. No new API calls.

---

### Task 1: Add fullscreen wizard CSS classes

**Files:**
- Modify: `web/src/styles/components.css:3628` (append after `.modal-footer .btn`)

Add the following CSS block after the existing `.modal-footer .btn` rule (after line 3628):

```css
/* === Fullscreen Wizard === */
.wizard-fullscreen {
  position: fixed;
  inset: 0;
  z-index: 1000;
  background: var(--bg);
  display: flex;
  flex-direction: column;
  animation: fadeIn 0.2s ease;
}

.wizard-fullscreen__header {
  display: flex;
  align-items: center;
  padding: 12px 24px;
  border-bottom: 1px solid var(--border);
  flex-shrink: 0;
  gap: 16px;
}

.wizard-fullscreen__back {
  display: inline-flex;
  align-items: center;
  gap: 6px;
  background: none;
  border: none;
  color: var(--muted);
  font-size: 13px;
  cursor: pointer;
  padding: 6px 10px;
  border-radius: 6px;
  transition: all 0.15s;
}

.wizard-fullscreen__back:hover {
  background: var(--bg-hover);
  color: var(--text);
}

.wizard-fullscreen__title {
  font-size: 15px;
  font-weight: 600;
  color: var(--text);
}

.wizard-fullscreen__steps {
  display: flex;
  gap: 6px;
  margin-left: auto;
}

.wizard-fullscreen__dot {
  width: 8px;
  height: 8px;
  border-radius: 50%;
  background: var(--border);
  transition: background 0.2s;
}

.wizard-fullscreen__dot--active {
  background: var(--primary, #3b82f6);
}

.wizard-fullscreen__dot--done {
  background: var(--success, #22c55e);
}

.wizard-fullscreen__body {
  flex: 1;
  overflow-y: auto;
  padding: 24px 32px;
}

/* Step 1: Template grid — 5 columns on wide screens */
.wizard-template-grid {
  display: grid;
  grid-template-columns: repeat(auto-fill, minmax(200px, 1fr));
  gap: 12px;
}

/* Step 2: Split panel */
.wizard-split {
  display: grid;
  grid-template-columns: 1fr 420px;
  height: 100%;
}

.wizard-split__form {
  overflow-y: auto;
  padding: 24px 32px;
}

.wizard-split__overview {
  overflow-y: auto;
  padding: 24px;
  border-left: 1px solid var(--border);
  background: var(--card);
}

/* Overview stats grid */
.wizard-overview__stats {
  display: grid;
  grid-template-columns: 1fr 1fr;
  gap: 10px;
  margin: 16px 0;
}

.wizard-overview__stat {
  padding: 12px;
  border-radius: 8px;
  background: var(--bg);
  text-align: center;
}

.wizard-overview__stat-value {
  font-size: 22px;
  font-weight: 700;
  color: var(--text);
}

.wizard-overview__stat-label {
  font-size: 12px;
  color: var(--muted);
  margin-top: 2px;
}

/* Overview section titles */
.wizard-overview__section-title {
  font-size: 13px;
  font-weight: 600;
  color: var(--text);
  margin: 20px 0 8px;
  text-transform: uppercase;
  letter-spacing: 0.03em;
}

/* Overview list items */
.wizard-overview__list {
  list-style: none;
  padding: 0;
  margin: 0;
}

.wizard-overview__list-item {
  display: flex;
  justify-content: space-between;
  align-items: center;
  padding: 6px 0;
  font-size: 13px;
  border-bottom: 1px solid var(--border);
}

.wizard-overview__list-item:last-child {
  border-bottom: none;
}

.wizard-overview__list-item-name {
  color: var(--text);
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
  min-width: 0;
  flex: 1;
}

.wizard-overview__list-item-meta {
  color: var(--muted);
  font-size: 11px;
  margin-left: 8px;
  white-space: nowrap;
}

/* Sticky form footer */
.wizard-form-footer {
  display: flex;
  justify-content: flex-end;
  gap: 12px;
  padding: 16px 32px;
  border-top: 1px solid var(--border);
  background: var(--bg);
  flex-shrink: 0;
  position: sticky;
  bottom: 0;
}
```

- [ ] **Step 1: Add CSS block after line 3628**

Append the CSS above after the existing `.modal-footer .btn` rule in `components.css`.

- [ ] **Step 2: Verify build**

Run: `cd /Users/chenguorong/code/my/tinyiothub/web && pnpm build`
Expected: Build succeeds with no CSS errors.

- [ ] **Step 3: Commit**

```bash
git add web/src/styles/components.css
git commit -m "feat(wizard): add fullscreen wizard CSS classes"
```

---

### Task 2: Replace `renderWizard()` — fullscreen container with header + step routing

**Files:**
- Modify: `web/src/ui/views/devices.ts:1093-1131` (replace `renderWizard()` method)

Replace the existing `renderWizard()` method (lines 1093-1131) with:

```typescript
  renderWizard() {
    const isStep1 = this.wizardStep === "template";
    return html`
      <div class="wizard-fullscreen">
        <!-- Header bar -->
        <div class="wizard-fullscreen__header">
          <button class="wizard-fullscreen__back" @click=${isStep1 ? this.closeWizard : this.wizardBack}>
            ${icons.arrowDown ? html`<span style="transform: rotate(90deg); display: inline-flex;">${icons.arrowDown}</span>` : "←"}
            <span>${isStep1 ? "返回设备列表" : "返回模板选择"}</span>
          </button>
          <span class="wizard-fullscreen__title">${isStep1 ? "选择设备模板" : "填写设备信息"}</span>
          <div class="wizard-fullscreen__steps">
            <div class="wizard-fullscreen__dot ${isStep1 ? 'wizard-fullscreen__dot--active' : 'wizard-fullscreen__dot--done'}"></div>
            <div class="wizard-fullscreen__dot ${!isStep1 ? 'wizard-fullscreen__dot--active' : ''}"></div>
          </div>
          ${isStep1 ? html`
            <button class="btn btn--ghost" @click=${this.closeWizard} style="margin-left: 8px;">取消</button>
          ` : nothing}
        </div>
        <!-- Body -->
        <div class="wizard-fullscreen__body">
          ${isStep1 ? this.renderWizardTemplateSelection() : this.renderWizardDeviceInfo()}
        </div>
      </div>
    `;
  }
```

- [ ] **Step 1: Replace renderWizard() method**

Edit `devices.ts` lines 1093-1131 with the code above.

- [ ] **Step 2: Verify build**

Run: `cd /Users/chenguorong/code/my/tinyiothub/web && pnpm build`
Expected: Build succeeds.

- [ ] **Step 3: Commit**

```bash
git add web/src/ui/views/devices.ts
git commit -m "feat(wizard): replace modal with fullscreen container"
```

---

### Task 3: Replace `renderWizardTemplateSelection()` — fullscreen template grid

**Files:**
- Modify: `web/src/ui/views/devices.ts:1133-1184` (replace `renderWizardTemplateSelection()`)

Replace the existing method with:

```typescript
  renderWizardTemplateSelection() {
    const groups = this.wizardTemplatesByCategory;
    const categories = Object.keys(groups);

    return html`
      <p style="text-align: center; color: var(--muted); font-size: 14px; margin: 0 0 20px;">
        选择一个设备模板来快速创建和配置您的IoT设备
      </p>
      <!-- Search bar centered, max 640px -->
      <div style="display: flex; justify-content: center; margin-bottom: 24px;">
        <div style="position: relative; width: 100%; max-width: 640px;">
          <span style="position: absolute; left: 12px; top: 50%; transform: translateY(-50%); color: var(--muted);">
            ${icons.search}
          </span>
          <input
            type="text"
            placeholder="搜索设备模板..."
            .value=${this.wizTemplateSearch}
            @input=${(e: Event) => { this.wizTemplateSearch = (e.target as HTMLInputElement).value; }}
            style="width: 100%; padding: 10px 14px 10px 38px; box-sizing: border-box; border-radius: 10px; border: 1px solid var(--border); background: var(--bg); color: var(--text); font-size: 14px;"
          />
        </div>
      </div>

      ${this.wizTemplateLoading ? html`
        <div style="display: flex; align-items: center; justify-content: center; padding: 60px;">
          <span class="loading-spinner"></span>
          <span style="margin-left: 8px; color: var(--muted);">加载中...</span>
        </div>
      ` : this.filteredWizardTemplates.length === 0 ? html`
        <div style="text-align: center; padding: 60px;">
          <div style="font-size: 48px; margin-bottom: 12px;">📦</div>
          <div style="font-size: 16px; font-weight: 500; color: var(--text);">没有找到匹配的模板</div>
          <div style="font-size: 13px; color: var(--muted); margin-top: 4px;">尝试调整搜索条件或浏览其他分类</div>
        </div>
      ` : html`
        ${categories.map(cat => html`
          <div style="margin-bottom: 28px;">
            <div style="display: flex; align-items: center; margin-bottom: 14px;">
              <span style="font-size: 16px; font-weight: 600;">${CATEGORY_LABELS[cat] || cat}</span>
              <span style="font-size: 12px; color: var(--muted); margin-left: 12px;">${groups[cat].length} 个模板</span>
            </div>
            <div class="wizard-template-grid">
              ${groups[cat].map(t => this.renderTemplateCard(t))}
            </div>
          </div>
        `)}
      `}
    `;
  }
```

Also replace `renderTemplateCard()` (lines 1186-1210) with a slightly larger card variant:

```typescript
  renderTemplateCard(t: ProcessedTemplate) {
    const displayName = getLocalizedText(t.displayName, t.name);
    return html`
      <div
        class="card"
        style="padding: 16px; cursor: pointer; transition: border-color 0.15s, box-shadow 0.15s;"
        @click=${() => this.selectTemplate(t)}
        @mouseenter=${(e: Event) => { (e.currentTarget as HTMLElement).style.borderColor = 'var(--primary, #3b82f6)'; (e.currentTarget as HTMLElement).style.boxShadow = '0 0 0 1px var(--primary, #3b82f6)'; }}
        @mouseleave=${(e: Event) => { (e.currentTarget as HTMLElement).style.borderColor = ''; (e.currentTarget as HTMLElement).style.boxShadow = ''; }}
      >
        <div style="display: flex; align-items: center; gap: 10px; margin-bottom: 10px;">
          <span style="font-size: 28px;">${CATEGORY_ICONS[t.category] || "📦"}</span>
          <div style="min-width: 0; flex: 1;">
            <div style="font-weight: 600; font-size: 14px; white-space: nowrap; overflow: hidden; text-overflow: ellipsis;">${displayName}</div>
            ${t.manufacturer ? html`<div style="font-size: 11px; color: var(--muted);">${t.manufacturer}</div>` : nothing}
          </div>
          ${t.isBuiltin ? html`<span style="font-size: 10px; padding: 1px 6px; border-radius: 4px; background: var(--bg-subtle); color: var(--muted); text-transform: uppercase;">内置</span>` : nothing}
        </div>
        <div style="display: flex; gap: 8px; font-size: 11px; color: var(--muted); flex-wrap: wrap;">
          ${t.deviceType ? html`<span>${t.deviceType}</span>` : nothing}
          ${t.protocolType ? html`<span>${t.protocolType}</span>` : nothing}
          ${t.version ? html`<span>v${t.version}</span>` : nothing}
        </div>
        <div style="display: flex; gap: 12px; font-size: 11px; color: var(--muted); margin-top: 8px;">
          <span>${t.properties.length} 属性</span>
          <span>${t.commands.length} 命令</span>
        </div>
      </div>
    `;
  }
```

- [ ] **Step 1: Replace renderWizardTemplateSelection() and renderTemplateCard()**

Edit both methods in `devices.ts`.

- [ ] **Step 2: Verify build**

Run: `cd /Users/chenguorong/code/my/tinyiothub/web && pnpm build`
Expected: Build succeeds.

- [ ] **Step 3: Commit**

```bash
git add web/src/ui/views/devices.ts
git commit -m "feat(wizard): fullscreen template selection with larger grid cards"
```

---

### Task 4: Replace `renderWizardDeviceInfo()` — split panel with form + overview

**Files:**
- Modify: `web/src/ui/views/devices.ts:1212-1324` (replace `renderWizardDeviceInfo()`)

Replace with a split-panel layout. Left side: the existing form. Right side: template overview (via new `renderTemplateOverview()`).

```typescript
  renderWizardDeviceInfo() {
    const t = this.wizSelectedTemplate;
    if (!t) return nothing;
    const displayName = getLocalizedText(t.displayName, t.name);
    const hasError = (name: string) => Boolean(this.wizValidationErrors[name]);
    const getError = (name: string) => this.wizValidationErrors[name] || "";

    return html`
      <div class="wizard-split">
        <!-- Left panel: form -->
        <div class="wizard-split__form">
          <div style="font-size: 14px; font-weight: 600; margin-bottom: 16px;">填写设备信息</div>

          <!-- Template summary chip -->
          <div style="display: flex; align-items: center; gap: 12px; padding: 12px 14px; border: 1px solid var(--border); border-radius: 10px; background: var(--bg-subtle); margin-bottom: 16px;">
            <span style="font-size: 24px;">${CATEGORY_ICONS[t.category] || "📦"}</span>
            <div style="min-width: 0; flex: 1;">
              <div style="font-weight: 600; font-size: 14px;">${displayName}</div>
              <div style="font-size: 12px; color: var(--muted); margin-top: 2px;">
                ${t.manufacturer ? html`<span>${t.manufacturer} · </span>` : nothing}
                <span>${t.deviceType || t.category}</span>
                ${t.version ? html` · v${t.version}` : nothing}
              </div>
            </div>
            ${t.isBuiltin ? html`<span style="font-size: 10px; padding: 2px 8px; border-radius: 4px; background: var(--bg); color: var(--muted); text-transform: uppercase;">内置</span>` : nothing}
          </div>

          <!-- Device name -->
          <div class="field">
            <span>设备名称 <span style="color: var(--danger);">*</span></span>
            <input
              type="text"
              placeholder="请输入设备名称"
              .value=${this.wizName}
              @input=${(e: any) => { this.wizName = e.target.value; }}
              style=${hasError("deviceName") ? "border-color: var(--danger);" : ""}
            />
            ${hasError("deviceName") ? html`<div style="font-size: 12px; color: var(--danger); margin-top: 4px;">${getError("deviceName")}</div>` : nothing}
          </div>

          <!-- Device description -->
          <div class="field" style="margin-top: 12px;">
            <span>设备描述 <span style="font-size: 11px; color: var(--muted);">(可选)</span></span>
            <textarea
              placeholder="请输入设备描述"
              rows="2"
              .value=${this.wizDescription}
              @input=${(e: any) => { this.wizDescription = e.target.value; }}
              style="resize: none;"
            ></textarea>
          </div>

          <!-- Device address -->
          <div class="field" style="margin-top: 12px;">
            <span>设备地址 ${isFieldRequired(t.deviceInfo, "address")
              ? html`<span style="color: var(--danger);">*</span>`
              : html`<span style="font-size: 11px; color: var(--muted);">(可选)</span>`}</span>
            <input
              type="text"
              placeholder="请输入设备IP地址或连接地址"
              .value=${this.wizAddress}
              @input=${(e: any) => { this.wizAddress = e.target.value; }}
              style=${hasError("deviceAddress") ? "border-color: var(--danger);" : ""}
            />
            ${hasError("deviceAddress") ? html`<div style="font-size: 12px; color: var(--danger); margin-top: 4px;">${getError("deviceAddress")}</div>` : nothing}
          </div>

          <!-- Device position -->
          <div class="field" style="margin-top: 12px;">
            <span>安装位置 <span style="font-size: 11px; color: var(--muted);">(可选)</span></span>
            <input
              type="text"
              placeholder="请输入设备安装位置"
              .value=${this.wizPosition}
              @input=${(e: any) => { this.wizPosition = e.target.value; }}
            />
          </div>

          <!-- Driver select -->
          <div class="field" style="margin-top: 12px;">
            <span>设备驱动 <span style="font-size: 11px; color: var(--muted);">(选择适合的驱动程序)</span></span>
            <select .value=${this.wizDriver} @change=${(e: Event) => this.onWizardDriverSelect((e.target as HTMLSelectElement).value)}>
              <option value="">请选择驱动</option>
              ${this.driverNames.map(name => html`<option value=${name}>${name}</option>`)}
            </select>
            ${t.driverName && this.wizDriver !== t.driverName ? html`
              <div style="font-size: 11px; color: var(--muted); margin-top: 4px;">模板默认驱动: ${t.driverName}</div>
            ` : nothing}
          </div>

          <!-- Driver config -->
          ${this.wizDriver ? html`
            <div style="margin-top: 16px;">
              <div style="display: flex; align-items: center; gap: 6px; margin-bottom: 12px;">
                <span style="font-size: 14px; font-weight: 600;">驱动配置</span>
                <span style="font-size: 12px; color: var(--muted);">(${this.wizDriver})</span>
              </div>
              ${this.wizConfigLoading ? html`
                <div style="display: flex; align-items: center; justify-content: center; padding: 20px;">
                  <span class="loading-spinner"></span>
                  <span style="margin-left: 8px; color: var(--muted);">加载驱动配置参数...</span>
                </div>
              ` : this.wizConfigOptions.length > 0 ? html`
                ${this.wizConfigOptions.map(opt => this.renderWizardConfigField(opt))}
              ` : html`
                <div style="padding: 12px; border: 1px solid var(--border); border-radius: 8px; color: var(--muted); font-size: 13px; text-align: center;">
                  该驱动无需额外配置参数
                </div>
              `}
            </div>
          ` : nothing}

          <!-- Bottom spacer for sticky footer -->
          <div style="height: 24px;"></div>
        </div>

        <!-- Right panel: template overview -->
        <div class="wizard-split__overview">
          ${this.renderTemplateOverview(t)}
        </div>
      </div>

      <!-- Sticky footer with action buttons -->
      <div class="wizard-form-footer">
        <button class="btn btn--ghost" @click=${this.wizardBack}>上一步</button>
        <button class="btn btn--primary" ?disabled=${this.wizardSaving || !this.wizName.trim()} @click=${this.submitWizard}>
          ${this.wizardSaving ? "创建中..." : "创建设备"}
        </button>
      </div>
    `;
  }
```

- [ ] **Step 1: Replace renderWizardDeviceInfo()**

Edit the method in `devices.ts`.

- [ ] **Step 2: Verify build**

Run: `cd /Users/chenguorong/code/my/tinyiothub/web && pnpm build`
Expected: Build succeeds (will fail on missing `renderTemplateOverview` — that's Task 5).

- [ ] **Step 3: Commit** (defer to Task 5 after both methods are in place)

---

### Task 5: Add `renderTemplateOverview()` — right panel with stats, properties, commands

**Files:**
- Modify: `web/src/ui/views/devices.ts` — add new method after `renderWizardConfigField()` (after line 1359)

Add this new method right before the closing `}` of the class:

```typescript
  renderTemplateOverview(t: ProcessedTemplate) {
    const displayName = getLocalizedText(t.displayName, t.name);

    // Compute stats from template properties
    const totalProps = t.properties.length;
    const totalCmds = t.commands.length;
    const readonlyProps = t.properties.filter((p: any) => p.accessMode === "r" || p.accessMode === "R").length;
    const writableProps = totalProps - readonlyProps;

    return html`
      <!-- Template summary -->
      <div style="display: flex; align-items: center; gap: 12px; margin-bottom: 20px;">
        <span style="font-size: 32px;">${CATEGORY_ICONS[t.category] || "📦"}</span>
        <div style="min-width: 0; flex: 1;">
          <div style="font-weight: 600; font-size: 16px;">${displayName}</div>
          <div style="font-size: 12px; color: var(--muted); margin-top: 2px;">
            ${t.manufacturer ? html`${t.manufacturer} · ` : nothing}${t.deviceType || t.category}${t.version ? html` · v${t.version}` : nothing}
          </div>
        </div>
        ${t.isBuiltin ? html`<span style="font-size: 10px; padding: 2px 8px; border-radius: 4px; background: var(--bg-subtle); color: var(--muted); text-transform: uppercase;">内置</span>` : nothing}
      </div>

      ${t.protocolType ? html`
        <div style="font-size: 12px; color: var(--muted); margin-bottom: 16px;">
          协议: ${t.protocolType}
        </div>
      ` : nothing}

      <!-- Stats grid 2x2 -->
      <div class="wizard-overview__stats">
        <div class="wizard-overview__stat">
          <div class="wizard-overview__stat-value">${totalProps}</div>
          <div class="wizard-overview__stat-label">属性数</div>
        </div>
        <div class="wizard-overview__stat">
          <div class="wizard-overview__stat-value">${totalCmds}</div>
          <div class="wizard-overview__stat-label">命令数</div>
        </div>
        <div class="wizard-overview__stat">
          <div class="wizard-overview__stat-value">${readonlyProps}</div>
          <div class="wizard-overview__stat-label">只读属性</div>
        </div>
        <div class="wizard-overview__stat">
          <div class="wizard-overview__stat-value">${writableProps}</div>
          <div class="wizard-overview__stat-label">可写属性</div>
        </div>
      </div>

      <!-- Property list -->
      ${totalProps > 0 ? html`
        <div class="wizard-overview__section-title">属性列表</div>
        <ul class="wizard-overview__list" style="max-height: 200px; overflow-y: auto;">
          ${t.properties.map((p: any) => html`
            <li class="wizard-overview__list-item">
              <span class="wizard-overview__list-item-name">${p.name || p.displayName || "unnamed"}</span>
              <span class="wizard-overview__list-item-meta">${p.dataType || ""}${p.unit ? ` ${p.unit}` : ""}</span>
            </li>
          `)}
        </ul>
      ` : nothing}

      <!-- Command list -->
      ${totalCmds > 0 ? html`
        <div class="wizard-overview__section-title">命令列表</div>
        <ul class="wizard-overview__list" style="max-height: 200px; overflow-y: auto;">
          ${t.commands.map((c: any) => html`
            <li class="wizard-overview__list-item">
              <span class="wizard-overview__list-item-name">${c.name || "unnamed"}</span>
              <span class="wizard-overview__list-item-meta">${c.description || ""}</span>
            </li>
          `)}
        </ul>
      ` : nothing}

      ${totalProps === 0 && totalCmds === 0 ? html`
        <div style="text-align: center; padding: 24px; color: var(--muted); font-size: 13px;">
          该模板暂无属性和命令定义
        </div>
      ` : nothing}
    `;
  }
```

- [ ] **Step 1: Add renderTemplateOverview() method**

Insert the method before the class closing brace.

- [ ] **Step 2: Verify build**

Run: `cd /Users/chenguorong/code/my/tinyiothub/web && pnpm build`
Expected: Build succeeds.

- [ ] **Step 3: Commit both Task 4 and Task 5 changes together**

```bash
git add web/src/ui/views/devices.ts
git commit -m "feat(wizard): split-panel step 2 with template overview"
```

---

### Task 6: Final build verification and visual check

- [ ] **Step 1: Full build**

Run: `cd /Users/chenguorong/code/my/tinyiothub/web && pnpm build`
Expected: Build succeeds with no errors.

- [ ] **Step 2: Verify no regressions in existing wizard methods**

Grep for `openWizard`, `closeWizard`, `selectTemplate`, `wizardBack`, `submitWizard` to confirm all still reference the correct methods and nothing was accidentally broken.

- [ ] **Step 3: Final commit (if any fixes needed)**

If no fixes needed, no commit required — previous commits already cover everything.

---

## Self-Review Checklist

1. **Spec coverage:**
   - Full-screen overlay for Step 1: Covered in Task 2 (CSS) + Task 3 (template grid)
   - Search + category grid with 5-6 columns: Covered in Task 3 (`grid-template-columns: repeat(auto-fill, minmax(200px, 1fr))`)
   - Split-panel Step 2 (1fr 420px): Covered in Task 4
   - Overview panel with stats grid 2x2: Covered in Task 5
   - Property list + command list in overview: Covered in Task 5
   - Independent scroll on right panel: Covered by `.wizard-split__overview { overflow-y: auto }`
   - Sticky footer with cancel/create buttons: Covered in Task 4
   - No extra API calls: Confirmed — all data from `ProcessedTemplate` already loaded

2. **Placeholder scan:** No TBD, TODO, or vague instructions found.

3. **Type consistency:** `ProcessedTemplate` properties (`properties`, `commands`, `category`, etc.) match usage in `renderTemplateOverview()`.

4. **Existing method preservation:** `openWizard()`, `closeWizard()`, `selectTemplate()`, `wizardBack()`, `onWizardDriverSelect()`, `validateWizardForm()`, `submitWizard()`, `renderWizardConfigField()` — all unchanged.
