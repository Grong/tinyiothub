import { LitElement, html } from "lit";
import { customElement, property } from "lit/decorators.js";

@customElement("home-panel")
export class HomePanel extends LitElement {
  @property({ type: String }) theme: "dark" | "light" = "dark";

  createRenderRoot() {
    return this;
  }

  handleMove(e: MouseEvent) {
    const target = e.currentTarget as HTMLElement;
    const rect = target.getBoundingClientRect();
    const x = e.clientX - rect.left;
    const y = e.clientY - rect.top;
    target.style.setProperty("--mouse-x", `${x}px`);
    target.style.setProperty("--mouse-y", `${y}px`);

    const cx = rect.width / 2;
    const cy = rect.height / 2;
    const rx = ((y - cy) / cy) * -3;
    const ry = ((x - cx) / cx) * 3;
    target.style.transform = `perspective(700px) rotateX(${rx}deg) rotateY(${ry}deg)`;
  }

  handleLeave(e: MouseEvent) {
    const target = e.currentTarget as HTMLElement;
    target.style.transform = "perspective(700px) rotateX(0deg) rotateY(0deg)";
  }

  render() {
    const isLight = this.theme === "light";

    return html`
      <style>
        home-panel {
          display: block;
        }

        home-panel .big-panel {
          position: relative;
          border-radius: 24px;
          background: ${isLight ? "rgba(255,255,255,0.6)" : "rgba(255,255,255,0.015)"};
          border: ${isLight ? "1px solid rgba(0,0,0,0.06)" : "none"};
          box-shadow:
            0 24px 80px rgba(0,0,0,${isLight ? "0.12" : "0.35"}),
            0 0 0 1px rgba(255,255,255,${isLight ? "0.5" : "0.04"}) inset;
          transform-style: preserve-3d;
          transition: transform 0.15s ease-out;
          overflow: hidden;
        }

        home-panel .big-panel__shine {
          position: absolute;
          inset: 0;
          border-radius: inherit;
          pointer-events: none;
          background: radial-gradient(
            600px circle at var(--mouse-x, 50%) var(--mouse-y, 50%),
            rgba(136, 59, 255, 0.12),
            transparent 40%
          );
          opacity: 0.8;
          z-index: 2;
        }

        home-panel .big-panel__content {
          position: relative;
          z-index: 3;
          display: grid;
          grid-template-columns: 1.2fr 1fr;
          gap: 48px;
          padding: 48px 56px;
          align-items: center;
        }

        home-panel .panel-tag {
          display: inline-flex;
          align-items: center;
          padding: 5px 12px;
          border-radius: 6px;
          font-size: 12px;
          font-weight: 600;
          color: ${isLight ? "rgba(0,0,0,0.7)" : "rgba(232,236,241,0.8)"};
          background: ${isLight ? "rgba(0,0,0,0.04)" : "rgba(255,255,255,0.05)"};
          margin-bottom: 24px;
        }

        home-panel .panel-stats-grid {
          display: grid;
          grid-template-columns: 1fr 1fr;
          gap: 28px 36px;
        }

        home-panel .panel-stat__num {
          font-size: 32px;
          font-weight: 800;
          color: ${isLight ? "#0f172a" : "#fff"};
          line-height: 1.1;
          margin-bottom: 4px;
        }

        home-panel .panel-stat__desc {
          font-size: 13px;
          color: ${isLight ? "rgba(0,0,0,0.5)" : "rgba(232,236,241,0.5)"};
          line-height: 1.4;
        }

        home-panel .big-panel__visual {
          display: flex;
          align-items: center;
          justify-content: center;
          min-height: 280px;
          position: relative;
        }

        home-panel .sphere-wrap {
          position: relative;
          width: 200px;
          height: 200px;
          display: flex;
          align-items: center;
          justify-content: center;
        }

        home-panel .sphere-core {
          width: 90px;
          height: 90px;
          border-radius: 50%;
          background: radial-gradient(circle at 30% 30%, rgba(0,212,255,0.35), rgba(0,152,255,0.15));
          box-shadow: 0 0 50px rgba(0,212,255,0.25);
          z-index: 1;
        }

        home-panel .sphere-glow {
          position: absolute;
          width: 260px;
          height: 260px;
          border-radius: 50%;
          background: radial-gradient(circle, rgba(136,59,255,0.12) 0%, transparent 55%);
          filter: blur(20px);
        }

        home-panel .sphere-ring {
          position: absolute;
          border-radius: 50%;
          border: 1px dashed rgba(0,212,255,0.18);
          animation: sphere-ring-rotate 10s linear infinite;
        }

        home-panel .sphere-ring--1 {
          width: 140px;
          height: 140px;
        }

        home-panel .sphere-ring--2 {
          width: 180px;
          height: 180px;
          border-color: rgba(123,97,255,0.12);
          animation-duration: 14s;
          animation-direction: reverse;
        }

        home-panel .sphere-ring--3 {
          width: 220px;
          height: 220px;
          border-color: rgba(0,212,255,0.08);
          animation-duration: 20s;
        }

        @keyframes sphere-ring-rotate {
          0% { transform: rotateX(70deg) rotateZ(0deg); }
          100% { transform: rotateX(70deg) rotateZ(360deg); }
        }

        @media (max-width: 1024px) {
          home-panel .big-panel__content {
            grid-template-columns: 1fr;
            gap: 48px;
          }
          home-panel .big-panel__visual {
            order: -1;
            min-height: 220px;
          }
        }

        @media (max-width: 768px) {
          home-panel .big-panel__content {
            padding: 32px 24px;
          }
          home-panel .panel-stats-grid {
            grid-template-columns: 1fr 1fr;
            gap: 20px 24px;
          }
          home-panel .panel-stat__num {
            font-size: 26px;
          }
        }
      </style>

      <div class="big-panel" @mousemove=${this.handleMove} @mouseleave=${this.handleLeave}>
        <div class="big-panel__shine"></div>
        <div class="big-panel__content">
          <div class="big-panel__stats">
            <div class="panel-tag">全球生态</div>
            <div class="panel-stats-grid">
              <div class="panel-stat">
                <div class="panel-stat__num">10,000+</div>
                <div class="panel-stat__desc">接入设备</div>
              </div>
              <div class="panel-stat">
                <div class="panel-stat__num">200+</div>
                <div class="panel-stat__desc">国家与地区</div>
              </div>
              <div class="panel-stat">
                <div class="panel-stat__num">4</div>
                <div class="panel-stat__desc">核心协议</div>
              </div>
              <div class="panel-stat">
                <div class="panel-stat__num">&lt;1天</div>
                <div class="panel-stat__desc">私有化部署</div>
              </div>
              <div class="panel-stat">
                <div class="panel-stat__num">L0-L3</div>
                <div class="panel-stat__desc">自愈等级</div>
              </div>
              <div class="panel-stat">
                <div class="panel-stat__num">开源</div>
                <div class="panel-stat__desc">社区支持</div>
              </div>
            </div>
          </div>
          <div class="big-panel__visual">
            <div class="sphere-wrap">
              <div class="sphere-glow"></div>
              <div class="sphere-ring sphere-ring--1"></div>
              <div class="sphere-ring sphere-ring--2"></div>
              <div class="sphere-ring sphere-ring--3"></div>
              <div class="sphere-core"></div>
            </div>
          </div>
        </div>
      </div>
    `;
  }
}
