import { html, type TemplateResult } from "lit";

const placeholderData: Record<string, { icon: string; desc: string }> = {
  files: { icon: "📁", desc: "Agent 可以访问和管理文件资源。此功能即将推出。" },
  skills: { icon: "🔧", desc: "Agent 可以扩展自定义技能来处理特定任务。此功能即将推出。" },
  channels: { icon: "📡", desc: "Agent 可以通过多种渠道（邮件、Webhook 等）发送通知。此功能即将推出。" },
  cron: { icon: "⏰", desc: "Agent 可以配置定时任务，按计划自动执行操作。此功能即将推出。" },
};

export function renderPlaceholder(panel: string): TemplateResult {
  const data = placeholderData[panel] || { icon: "⚡", desc: "此功能即将推出。" };
  return html`
    <div class="agent-placeholder">
      <div class="agent-placeholder__icon">${data.icon}</div>
      <p class="agent-placeholder__desc">${data.desc}</p>
    </div>
  `;
}
