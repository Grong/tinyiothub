import { LitElement, html} from 'lit'
import { customElement, state } from 'lit/decorators.js'
import { navigate } from '../lib/navigate'

@customElement('home-page')
export class HomePage extends LitElement {
  createRenderRoot() { return this }
  

  render() {
    return html`
      <!-- Hero -->
      <section class="hero">
        <div class="hero-badge">
          <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
            <path stroke-linecap="round" stroke-linejoin="round" d="M9.813 15.904L9 18.75l-.813-2.846a4.5 4.5 0 00-3.09-3.09L2.25 12l2.846-.813a4.5 4.5 0 003.09-3.09L9 5.25l.813 2.846a4.5 4.5 0 003.09 3.09L15.75 12l-2.846.813a4.5 4.5 0 00-3.09 3.09zM18.259 8.715L18 9.75l-.259-1.035a3.375 3.375 0 00-2.455-2.456L14.25 6l1.036-.259a3.375 3.375 0 002.455-2.456L18 2.25l.259 1.035a3.375 3.375 0 002.456 2.456L21.75 6l-1.035.259a3.375 3.375 0 00-2.456 2.456zM16.894 20.567L16.5 21.75l-.394-1.183a2.25 2.25 0 00-1.423-1.423L13.5 18.75l1.183-.394a2.25 2.25 0 001.423-1.423l.394-1.183.394 1.183a2.25 2.25 0 001.423 1.423l1.183.394-1.183.394a2.25 2.25 0 00-1.423 1.423z"/>
          </svg>
          Edge Intelligence Agent
        </div>
        <h1 class="hero-title">
          <span class="gradient">智能边缘网关</span><br/>
          连接万物，驱动未来
        </h1>
        <p class="hero-desc">
          TinyIoTHub 是一款面向工业物联网的高性能边缘计算网关平台，支持多协议设备接入、实时数据处理与智能告警
        </p>
        <div class="hero-btns">
          <button class="btn primary" @click=${() => navigate('tenant/register')}>
            立即体验
            <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
              <path stroke-linecap="round" stroke-linejoin="round" d="M13.5 4.5L21 12m0 0l-7.5 7.5M21 12H3"/>
            </svg>
          </button>
          <button class="btn secondary" @click=${() => navigate('signin')}>
            登录控制台
          </button>
        </div>
      </section>

      <!-- Protocols -->
      <section class="protocols">
        <div class="protocols-inner">
          <div class="protocol-badge">
            <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
              <path stroke-linecap="round" stroke-linejoin="round" d="M5.25 14.25h13.5m-13.5 0a3 3 0 01-3-3m3 3a3 3 0 100 6h13.5a3 3 0 100-6m-16.5-3a3 3 0 013-3h13.5a3 3 0 013 3m-19.5 0a4.5 4.5 0 01.9-2.7L5.737 5.1a3.375 3.375 0 012.7-1.35h7.126c1.062 0 2.062.5 2.7 1.35l2.587 3.45a4.5 4.5 0 01.9 2.7m0 0a3 3 0 01-3 3m0 3h.008v.008h-.008v-.008zm0-6h.008v.008h-.008v-.008zm-3 6h.008v.008h-.008v-.008zm0-6h.008v.008h-.008v-.008z"/>
            </svg>
            Modbus
          </div>
          <div class="protocol-badge">
            <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
              <path stroke-linecap="round" stroke-linejoin="round" d="M15 10.5a3 3 0 11-6 0 3 3 0 016 0z"/>
              <path stroke-linecap="round" stroke-linejoin="round" d="M19.5 10.5c0 7.142-7.5 11.25-7.5 11.25S4.5 17.642 4.5 10.5a7.5 7.5 0 1115 0z"/>
            </svg>
            ONVIF
          </div>
          <div class="protocol-badge">
            <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
              <path stroke-linecap="round" stroke-linejoin="round" d="M8.288 15.038a5.25 5.25 0 017.424 0M5.106 11.856c3.807-3.808 9.98-3.808 13.788 0M1.924 8.674c5.565-5.565 14.587-5.565 20.152 0M12.53 18.22l-.53.53-.53-.53a.75.75 0 011.06 0z"/>
            </svg>
            SNMP
          </div>
          <div class="protocol-badge">
            <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
              <path stroke-linecap="round" stroke-linejoin="round" d="M8.25 3v1.5M4.5 8.25H3m18 0h-1.5M4.5 12H3m18 0h-1.5m-15 3.75H3m18 0h-1.5M8.25 19.5V21M12 3v1.5m0 15V21m3.75-18v1.5m0 15V21m-9-1.5h10.5a2.25 2.25 0 002.25-2.25V6.75a2.25 2.25 0 00-2.25-2.25H6.75A2.25 2.25 0 004.5 6.75v10.5a2.25 2.25 0 002.25 2.25zm.75-12h9v9h-9v-9z"/>
            </svg>
            MQTT
          </div>
        </div>
      </section>

      <!-- Stats -->
      <section class="stats">
        <div class="stats-inner">
          <div class="stat-item">
            <div class="stat-value">99.99%</div>
            <div class="stat-label">系统可用性</div>
          </div>
          <div class="stat-item">
            <div class="stat-value">9999+</div>
            <div class="stat-label">并发设备接入</div>
          </div>
          <div class="stat-item">
            <div class="stat-value">&lt;50ms</div>
            <div class="stat-label">平均响应延迟</div>
          </div>
          <div class="stat-item">
            <div class="stat-value">7×24</div>
            <div class="stat-label">全天候监控</div>
          </div>
        </div>
      </section>

      <!-- Features -->
      <section class="features">
        <div class="section-header">
          <span class="section-tag">核心功能</span>
          <h2 class="section-title">为什么选择 TinyIoTHub？</h2>
          <p class="section-desc">
            强大的边缘计算能力，多协议支持，以及智能化的运维管理
          </p>
        </div>
        <div class="features-grid">
          <div class="feature-card">
            <div class="feature-icon">
              <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
                <path stroke-linecap="round" stroke-linejoin="round" d="M3.75 13.5l10.5-11.25L12 10.5h8.25L9.75 21.75 12 13.5H3.75z"/>
              </svg>
            </div>
            <h3 class="feature-title">边缘计算</h3>
            <p class="feature-desc">支持在边缘网关本地执行数据处理、协议转换和逻辑运算，减少云端依赖，降低延迟</p>
          </div>
          <div class="feature-card">
            <div class="feature-icon">
              <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
                <path stroke-linecap="round" stroke-linejoin="round" d="M7.5 21L3 16.5m0 0L7.5 12M3 16.5h13.5m0-13.5L21 7.5m0 0L16.5 12M21 7.5H7.5"/>
              </svg>
            </div>
            <h3 class="feature-title">多协议适配</h3>
            <p class="feature-desc">同时支持 Modbus、ONVIF、SNMP、MQTT 等主流工业协议，灵活适配各种设备</p>
          </div>
          <div class="feature-card">
            <div class="feature-icon">
              <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
                <path stroke-linecap="round" stroke-linejoin="round" d="M3 13.125C3 12.504 3.504 12 4.125 12h2.25c.621 0 1.125.504 1.125 1.125v6.75C7.5 20.496 6.996 21 6.375 21h-2.25A1.125 1.125 0 013 19.875v-6.75zM9.75 8.625c0-.621.504-1.125 1.125-1.125h2.25c.621 0 1.125.504 1.125 1.125v11.25c0 .621-.504 1.125-1.125 1.125h-2.25a1.125 1.125 0 01-1.125-1.125V8.625zM16.5 4.125c0-.621.504-1.125 1.125-1.125h2.25C20.496 3 21 3.504 21 4.125v15.75c0 .621-.504 1.125-1.125 1.125h-2.25a1.125 1.125 0 01-1.125-1.125V4.125z"/>
              </svg>
            </div>
            <h3 class="feature-title">实时监控</h3>
            <p class="feature-desc">可视化仪表盘，实时展示设备状态、数据趋势和告警信息，掌握全局动态</p>
          </div>
          <div class="feature-card">
            <div class="feature-icon">
              <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
                <path stroke-linecap="round" stroke-linejoin="round" d="M14.857 17.082a23.848 23.848 0 005.454-1.31A8.967 8.967 0 0118 9.75v-.7V9A6 6 0 006 9v.75a8.967 8.967 0 01-2.312 6.022c1.733.64 3.56 1.085 5.455 1.31m5.714 0a24.255 24.255 0 01-5.714 0m5.714 0a3 3 0 11-5.714 0"/>
              </svg>
            </div>
            <h3 class="feature-title">智能告警</h3>
            <p class="feature-desc">多级别告警规则，灵活的通知策略，支持邮件、短信、Webhook 等多种方式</p>
          </div>
        </div>
      </section>

      <!-- AI Agents -->
      <section class="agents">
        <div class="section-header">
          <span class="section-tag">AI Agent</span>
          <h2 class="section-title">智能运维助手</h2>
          <p class="section-desc">
            基于大语言模型的智能助手，帮助您更高效地管理和运维 IoT 设备
          </p>
        </div>
        <div class="agents-grid">
          <div class="agent-card">
            <div class="agent-header">
              <div class="agent-avatar">🔧</div>
              <span class="agent-name">设备诊断助手</span>
            </div>
            <p class="agent-desc">自动分析设备异常，提供诊断建议和解决方案</p>
          </div>
          <div class="agent-card">
            <div class="agent-header">
              <div class="agent-avatar">📊</div>
              <span class="agent-name">数据分析师</span>
            </div>
            <p class="agent-desc">智能分析设备数据，发现趋势和潜在问题</p>
          </div>
          <div class="agent-card">
            <div class="agent-header">
              <div class="agent-avatar">⚡</div>
              <span class="agent-name">性能优化师</span>
            </div>
            <p class="agent-desc">监控系统性能，提供优化建议和容量规划</p>
          </div>
          <div class="agent-card">
            <div class="agent-header">
              <div class="agent-avatar">🔒</div>
              <span class="agent-name">安全审计员</span>
            </div>
            <p class="agent-desc">实时监控安全事件，及时发现和响应威胁</p>
          </div>
          <div class="agent-card">
            <div class="agent-header">
              <div class="agent-avatar">📝</div>
              <span class="agent-name">日志分析员</span>
            </div>
            <p class="agent-desc">自动分析日志数据，快速定位问题根因</p>
          </div>
          <div class="agent-card">
            <div class="agent-header">
              <div class="agent-avatar">🤖</div>
              <span class="agent-name">自动化工程师</span>
            </div>
            <p class="agent-desc">编排自动化任务，减少人工干预和重复工作</p>
          </div>
        </div>
      </section>

      <!-- CTA -->
      <section class="cta">
        <div class="cta-inner">
          <h2 class="cta-title">立即开始使用</h2>
          <p class="cta-desc">
            几分钟内完成注册，即可体验完整的 IoT 边缘网关功能
          </p>
          <div class="hero-btns">
            <button class="btn primary" @click=${() => navigate('tenant/register')}>
              免费试用
              <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
                <path stroke-linecap="round" stroke-linejoin="round" d="M13.5 4.5L21 12m0 0l-7.5 7.5M21 12H3"/>
              </svg>
            </button>
          </div>
        </div>
      </section>

      <!-- Footer -->
      <footer class="footer">
        <div class="footer-inner">
          <div class="footer-brand">
            <div class="footer-logo">
              <logo-icon size="28px"></logo-icon>
              <span class="footer-logo-text">TinyIoTHub</span>
            </div>
            <p class="footer-tagline">
              高性能 IoT 边缘网关平台，支持多协议设备接入与智能运维
            </p>
          </div>
          <div class="footer-links">
            <div class="footer-col">
              <h4>产品</h4>
              <ul>
                <li><a href="/marketplace">市场</a></li>
                <li><a href="https://docs.tinyiothub.com" target="_blank" rel="noopener">文档</a></li>
                <li><a href="/pricing">定价</a></li>
              </ul>
            </div>
            <div class="footer-col">
              <h4>支持</h4>
              <ul>
                <li><a href="/help">帮助中心</a></li>
                <li><a href="/contact">联系我们</a></li>
                <li><a href="/faq">常见问题</a></li>
              </ul>
            </div>
            <div class="footer-col">
              <h4>资源</h4>
              <ul>
                <li><a href="/blog">博客</a></li>
                <li><a href="/github">GitHub</a></li>
                <li><a href="/community">社区</a></li>
              </ul>
            </div>
          </div>
        </div>
        <div class="footer-bottom">
          <span class="footer-copyright">© 2024 TinyIoTHub. 京ICP备XXXXXXXX号-1</span>
          <div class="footer-legal">
            <a href="/privacy">隐私政策</a>
            <a href="/terms">服务条款</a>
          </div>
        </div>
      </footer>
    `
  }
}

declare global {
  interface HTMLElementTagNameMap {
    'home-page': HomePage
  }
}
