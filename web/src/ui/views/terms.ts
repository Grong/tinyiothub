import { LitElement, html } from "lit";
import { customElement } from "lit/decorators.js";

@customElement("view-terms")
export class TermsView extends LitElement {
  createRenderRoot() {
    return this;
  }

  goToPrivacy(e: Event) {
    e.preventDefault();
    window.history.pushState({}, "", "/privacy");
    window.dispatchEvent(new PopStateEvent("popstate"));
  }

  render() {
    return html`
      <div class="legal-page">
        <article class="legal-doc">
          <h1 class="legal-title">服务条款</h1>
          <p class="legal-meta">最后更新:2026-05-06</p>

          <section>
            <h2>1. 接受条款</h2>
            <p>
              欢迎使用 TinyIoTHub(以下简称"本服务")。本服务由 TinyIoTHub 团队提供。
              在使用本服务前,请您仔细阅读本服务条款(以下简称"本条款")。当您完成注册或开始使用本服务时,
              即视为您已阅读、理解并同意本条款全部内容。
            </p>
          </section>

          <section>
            <h2>2. 服务说明</h2>
            <p>
              本服务为面向工业物联网场景的 SaaS 平台,提供边缘网关接入、多协议设备管理、
              实时数据监控、规则告警、AI Agent 集成等能力。我们可能会不时增加、调整或下线部分功能。
            </p>
          </section>

          <section>
            <h2>3. 账户注册与安全</h2>
            <ul>
              <li>您应使用真实手机号注册并妥善保管账户与密码;</li>
              <li>账户仅限本人使用,不得转让、出售或共享;</li>
              <li>因您未妥善保管账户造成的损失由您本人承担;</li>
              <li>如发现账户被盗用或异常,请立即联系我们。</li>
            </ul>
          </section>

          <section>
            <h2>4. 用户行为规范</h2>
            <p>您承诺不会利用本服务从事以下活动:</p>
            <ul>
              <li>违反中国法律法规或损害国家利益、社会公共利益的行为;</li>
              <li>侵犯他人知识产权、商业秘密或其他合法权益;</li>
              <li>上传、传播恶意代码、病毒或对系统进行攻击、渗透;</li>
              <li>利用本服务对未授权设备进行接入、控制或数据采集。</li>
            </ul>
          </section>

          <section>
            <h2>5. 数据所有权</h2>
            <p>
              您通过本服务接入设备所产生的业务数据归您所有,我们仅作为数据处理者,
              按照《<a href="/privacy" @click=${this.goToPrivacy}>隐私政策</a>》和您的指令进行存储、计算和传输。
            </p>
          </section>

          <section>
            <h2>6. 服务可用性</h2>
            <p>
              我们将尽合理努力维持服务的可用性,但因系统维护、网络故障、不可抗力等原因导致的服务中断,
              我们不承担违约责任。重要业务请做好本地容灾。
            </p>
          </section>

          <section>
            <h2>7. 责任限制</h2>
            <p>
              在适用法律允许的最大范围内,我们对因使用或无法使用本服务造成的任何间接、附带、
              特殊或后果性损失不承担责任。
            </p>
          </section>

          <section>
            <h2>8. 条款变更</h2>
            <p>
              我们可能根据法规或业务调整修订本条款,变更后我们会通过站内消息或邮件通知您。
              如您不同意变更内容,可停止使用本服务;继续使用即视为接受变更后的条款。
            </p>
          </section>

          <section>
            <h2>9. 联系我们</h2>
            <p>
              如对本条款有任何疑问,请通过 <code>support@tinyiothub.example</code> 联系我们。
            </p>
          </section>
        </article>
      </div>
    `;
  }
}
