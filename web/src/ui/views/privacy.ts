import { LitElement, html } from "lit";
import { customElement } from "lit/decorators.js";

@customElement("view-privacy")
export class PrivacyView extends LitElement {
  createRenderRoot() {
    return this;
  }

  goToTerms(e: Event) {
    e.preventDefault();
    window.history.pushState({}, "", "/terms");
    window.dispatchEvent(new PopStateEvent("popstate"));
  }

  render() {
    return html`
      <div class="legal-page">
        <article class="legal-doc">
          <h1 class="legal-title">隐私政策</h1>
          <p class="legal-meta">最后更新:2026-05-06</p>

          <section>
            <p>
              我们(TinyIoTHub 团队)非常重视您的个人信息和数据安全。本《隐私政策》说明我们在您使用 TinyIoTHub 服务过程中,
              如何收集、使用、存储、共享和保护您的个人信息以及设备数据,以及您依法享有的权利。
              请您仔细阅读并理解本政策,继续使用本服务即视为您同意本政策。
            </p>
          </section>

          <section>
            <h2>1. 我们收集的信息</h2>
            <h3>1.1 您主动提供的信息</h3>
            <ul>
              <li><strong>账户信息:</strong>用户名、手机号、密码(加密存储)、邮箱(选填)、显示名;</li>
              <li><strong>设备配置:</strong>您接入的设备名称、协议参数、采集点表、规则表达式等;</li>
              <li><strong>反馈信息:</strong>您主动提交的工单、问题反馈或客服沟通内容。</li>
            </ul>
            <h3>1.2 服务运行中产生的信息</h3>
            <ul>
              <li><strong>设备数据:</strong>从您接入的设备采集到的实时点位、事件、告警等业务数据;</li>
              <li><strong>使用日志:</strong>登录时间、IP 地址、操作记录、API 调用日志(用于安全审计);</li>
              <li><strong>设备/浏览器信息:</strong>UA、屏幕尺寸、语言偏好(用于优化界面体验)。</li>
            </ul>
          </section>

          <section>
            <h2>2. 我们如何使用信息</h2>
            <ul>
              <li>提供、维护和优化 TinyIoTHub 服务及其各项功能;</li>
              <li>账户身份验证、安全风控、防止欺诈与滥用;</li>
              <li>根据您的设备配置执行规则引擎、告警通知和自动化任务;</li>
              <li>履行法律法规规定的合规义务(如必要的日志留存);</li>
              <li>在征得您同意后,向您发送服务通知或产品更新信息。</li>
            </ul>
          </section>

          <section>
            <h2>3. 信息存储与跨境</h2>
            <p>
              您的个人信息及设备数据存储于中国境内的服务器集群。除法律法规明确要求或您主动选择的跨境同步功能外,
              我们不会将您的数据传输至中华人民共和国境外。
            </p>
          </section>

          <section>
            <h2>4. 信息共享</h2>
            <p>除以下情形外,我们不会与第三方共享您的个人信息:</p>
            <ul>
              <li>已获得您的明确同意;</li>
              <li>为履行法律法规、行政、司法机关的强制要求;</li>
              <li>与提供基础服务的合作方共享必要信息(如短信验证码网关、对象存储),并通过协议约束其保密义务。</li>
            </ul>
          </section>

          <section>
            <h2>5. 信息安全</h2>
            <ul>
              <li>密码采用 Argon2/bcrypt 等单向哈希算法存储,我们无法获知您的明文密码;</li>
              <li>API 通信采用 HTTPS/TLS 加密;</li>
              <li>采用 JWT + 多租户隔离机制,确保不同工作空间的数据互不可见;</li>
              <li>定期开展安全审计与漏洞扫描。</li>
            </ul>
          </section>

          <section>
            <h2>6. 您的权利</h2>
            <p>根据《个人信息保护法》及相关法律,您对您的个人信息享有以下权利:</p>
            <ul>
              <li>查询、复制权:可在"账户设置"中查看您的个人信息;</li>
              <li>更正、补充权:可随时修改不准确的信息;</li>
              <li>删除权:您可注销账户,届时我们将删除或匿名化您的个人信息;</li>
              <li>撤回同意权:您可随时撤回此前同意的非必要授权。</li>
            </ul>
          </section>

          <section>
            <h2>7. Cookie 与本地存储</h2>
            <p>
              我们使用浏览器 LocalStorage 存储您的登录令牌、UI 偏好(主题、语言等),
              不会用于跨站追踪或行为画像。您可随时在浏览器中清除。
            </p>
          </section>

          <section>
            <h2>8. 政策变更</h2>
            <p>
              我们可能会不时修订本政策。重大变更时我们会通过站内消息或邮件通知您。
              本政策与《<a href="/terms" @click=${this.goToTerms}>服务条款</a>》共同构成您与我们之间的完整协议。
            </p>
          </section>

          <section>
            <h2>9. 联系我们</h2>
            <p>
              如您对本政策或您的个人信息处理有任何疑问、投诉或建议,
              请通过 <code>privacy@tinyiothub.example</code> 与我们联系,我们将在 15 个工作日内回复。
            </p>
          </section>
        </article>
      </div>
    `;
  }
}
