import { html, type TemplateResult } from "lit";

export function renderA2uiTabs(data: Record<string, unknown>, onAction?: (fn: string, args: Record<string, unknown>) => void): TemplateResult {
  const tabs = (data.tabs as Array<{ id: string; label: string }>) || [];
  const activeTab = String(data.activeTab || tabs[0]?.id || "");

  return html`
    <div class="a2ui-tabs">
      <div class="a2ui-tabs__header">
        ${tabs.map((tab) => html`
          <button
            class="a2ui-tabs__tab ${tab.id === activeTab ? "a2ui-tabs__tab--active" : ""}"
            @click=${() => { if (onAction) onAction("selectTab", { tabId: tab.id }); }}
          >${tab.label}</button>
        `)}
      </div>
    </div>
  `;
}
