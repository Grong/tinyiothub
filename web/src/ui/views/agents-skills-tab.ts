import { html, nothing, type TemplateResult } from "lit";
import { type AgentsState } from "../controllers/agents.js";
import { loadSkills, removeSkill, createSkillApi, updateSkillApi } from "../controllers/agents.js";

export function renderSkillsTab(
  state: AgentsState,
  patchState: (patch: Partial<AgentsState>) => void,
  onSave: () => void,
): TemplateResult {
  const skills = state.skillsList || [];
  const loading = state.skillsLoading;
  const error = state.skillsError;
  const panel = state.activeSkillsPanel || "list";
  const draft = state.skillDraft || "";
  const editing = state.editingSkill;

  if (loading && skills.length === 0) {
    return html`<div style="padding: 20px; text-align: center; color: var(--muted);">加载技能...</div>`;
  }

  if (error) {
    return html`<div style="padding: 20px; color: var(--error);">${error}</div>`;
  }

  // List view
  if (panel === "list") {
    return html`
      <div class="skills-panel">
        <div class="skills-panel__header">
          <span>${skills.length} 个技能</span>
          <button class="a2ui-btn a2ui-btn--primary a2ui-btn--sm"
                  @click=${() => patchState({ activeSkillsPanel: "create", skillDraft: "---\nname: \ndescription: \n---\n\n" })}>
            + 新建技能
          </button>
        </div>
        <div class="skills-list">
          ${skills.length === 0 ? html`<div class="skills-empty">暂无技能，点击"新建技能"创建</div>` : nothing}
          ${skills.map((skill) => html`
            <div class="skill-item">
              <div class="skill-item__info">
                <div class="skill-item__name">${skill.name}</div>
                <div class="skill-item__desc">${skill.description}</div>
              </div>
              <div class="skill-item__actions">
                <button class="a2ui-btn a2ui-btn--secondary a2ui-btn--sm"
                        @click=${() => patchState({ activeSkillsPanel: "edit", editingSkill: skill, skillDraft: skill.content })}>
                  编辑
                </button>
                <button class="a2ui-btn a2ui-btn--secondary a2ui-btn--sm"
                        aria-label="删除技能"
                        @click=${() => patchState({ pendingDelete: skill.name })}>
                  删除
                </button>
              </div>
            </div>
          `)}
        </div>
      </div>
    `;
  }

  // Editor view (create or edit)
  if (panel === "create" || panel === "edit") {
    return html`
      <div class="skills-editor">
        <div class="skills-editor__header">
          <span>${panel === "create" ? "新建技能" : `编辑: ${editing?.name}`}</span>
          <div style="display: flex; gap: 8px;">
            <button class="a2ui-btn a2ui-btn--secondary a2ui-btn--sm"
                    @click=${() => patchState({ activeSkillsPanel: "list", editingSkill: null, skillDraft: "" })}>
              取消
            </button>
            <button class="a2ui-btn a2ui-btn--primary a2ui-btn--sm"
                    @click=${async () => {
                      if (panel === "create") {
                        const data = { workspace_id: state.config?.workspace || "tinyiothub", skill_name: "untitled", skill_content: draft };
                        const ok = await createSkillApi(state, data);
                        if (ok) { patchState({ activeSkillsPanel: "list", editingSkill: null, skillDraft: "" }); onSave(); }
                      } else {
                        const data = { skill_content: draft };
                        const ok = await updateSkillApi(state, editing?.name, data);
                        if (ok) { patchState({ activeSkillsPanel: "list", editingSkill: null, skillDraft: "" }); onSave(); }
                      }
                    }}>
              保存
            </button>
          </div>
        </div>
        <textarea class="skills-editor__textarea"
                .value=${draft}
                @input=${(e: Event) => patchState({ skillDraft: (e.target as HTMLTextAreaElement).value })}
                placeholder="---&#10;name: skill-name&#10;description: 技能描述&#10;---&#10;&#10;技能内容 (Markdown)..."
                spellcheck="false"
                tabindex="0"></textarea>
      </div>
    `;
  }

  // Delete confirmation modal
  if (state.pendingDelete) {
    return html`
      <div class="modal-overlay" @click=${(e: Event) => { if (e.target === e.currentTarget) patchState({ pendingDelete: null }); }}>
        <div class="modal-box">
          <div class="modal-header">
            <h3>确认删除</h3>
            <button class="modal-close" @click=${() => patchState({ pendingDelete: null })}>×</button>
          </div>
          <div class="modal-body">
            <p class="modal-desc">确定要删除技能「${state.pendingDelete}」吗？此操作不可撤销。</p>
          </div>
          <div class="modal-footer">
            <button class="btn-secondary" @click=${() => patchState({ pendingDelete: null })}>取消</button>
            <button class="a2ui-btn a2ui-btn--danger a2ui-btn--sm"
                    aria-label="确认删除技能"
                    @click=${async () => {
                      await removeSkill(state, state.pendingDelete!);
                      patchState({ pendingDelete: null });
                      onSave();
                    }}>
              删除
            </button>
          </div>
        </div>
      </div>
    `;
  }

  return nothing;
}
