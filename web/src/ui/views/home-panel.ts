import { LitElement, html } from "lit";
import { customElement, property, query } from "lit/decorators.js";

@customElement("home-panel")
export class HomePanel extends LitElement {
  @property({ type: String }) theme: "dark" | "light" = "dark";

  @query(".big-panel__content") contentEl!: HTMLElement;
  @query(".big-panel__visual") visualEl!: HTMLElement;
  @query("slot[name=visual]") visualSlot!: HTMLSlotElement;

  firstUpdated() {
    this.updateLayout();
    this.visualSlot.addEventListener("slotchange", this.updateLayout);
  }

  private updateLayout = () => {
    const hasVisual = this.visualSlot.assignedElements().length > 0;
    if (hasVisual) {
      this.contentEl.style.gridTemplateColumns = "";
      this.visualEl.style.display = "";
    } else {
      this.contentEl.style.gridTemplateColumns = "1fr";
      this.visualEl.style.display = "none";
    }
  };

  handleMove(e: MouseEvent) {
    const target = e.currentTarget as HTMLElement;
    const rect = target.getBoundingClientRect();
    const x = e.clientX - rect.left;
    const y = e.clientY - rect.top;
    target.style.setProperty("--mouse-x", `${x}px`);
    target.style.setProperty("--mouse-y", `${y}px`);

    const cx = rect.width / 2;
    const cy = rect.height / 2;
    const rx = ((y - cy) / cy) * -1.5;
    const ry = ((x - cx) / cx) * 1.5;
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
        :host {
          display: block;
        }

        .big-panel {
          position: relative;
          border-radius: 24px;
          background: ${isLight ? "rgba(255,255,255,0.65)" : "rgba(10,14,22,0.85)"};
          border: ${isLight ? "1px solid rgba(0,0,0,0.04)" : "none"};
          box-shadow:
            0 4px 20px rgba(0,0,0,${isLight ? "0.05" : "0.35"}),
            0 16px 60px rgba(0,0,0,${isLight ? "0.06" : "0.28"}),
            0 40px 100px rgba(0,212,255,${isLight ? "0.04" : "0.04"}),
            0 0 0 1px rgba(255,255,255,${isLight ? "0.5" : "0.04"}) inset;
          transform-style: preserve-3d;
          transition: transform 0.15s ease-out;
          overflow: hidden;
        }

        .big-panel__shine {
          position: absolute;
          inset: 0;
          border-radius: inherit;
          pointer-events: none;
          background: radial-gradient(
            800px circle at var(--mouse-x, 50%) var(--mouse-y, 50%),
            rgba(136, 59, 255, 0.08),
            transparent 45%
          );
          opacity: 0.6;
          z-index: 2;
        }

        .big-panel__content {
          position: relative;
          z-index: 3;
          display: grid;
          grid-template-columns: 1.2fr 1fr;
          gap: 48px;
          padding: 48px 56px;
          align-items: center;
        }

        .panel-tag {
          display: inline-flex;
          align-items: center;
          padding: 5px 12px;
          border-radius: 6px;
          font-size: 12px;
          font-weight: 600;
          color: ${isLight ? "rgba(0,0,0,0.7)" : "rgba(232,236,241,0.9)"};
          background: ${isLight ? "rgba(0,0,0,0.04)" : "rgba(255,255,255,0.08)"};
          margin-bottom: 24px;
        }

        .big-panel__visual {
          display: flex;
          align-items: center;
          justify-content: center;
          min-height: 280px;
          position: relative;
        }

        .sphere-wrap {
          position: relative;
          width: 200px;
          height: 200px;
          display: flex;
          align-items: center;
          justify-content: center;
        }

        .sphere-core {
          width: 90px;
          height: 90px;
          border-radius: 50%;
          background: radial-gradient(circle at 30% 30%, rgba(0,212,255,0.35), rgba(0,152,255,0.15));
          box-shadow: 0 0 50px rgba(0,212,255,0.25);
          z-index: 1;
        }

        .sphere-glow {
          position: absolute;
          width: 260px;
          height: 260px;
          border-radius: 50%;
          background: radial-gradient(circle, rgba(136,59,255,0.12) 0%, transparent 55%);
          filter: blur(20px);
        }

        .sphere-ring {
          position: absolute;
          border-radius: 50%;
          border: 1px dashed rgba(0,212,255,0.18);
          animation: sphere-ring-rotate 10s linear infinite;
        }

        .sphere-ring--1 {
          width: 140px;
          height: 140px;
        }

        .sphere-ring--2 {
          width: 180px;
          height: 180px;
          border-color: rgba(123,97,255,0.12);
          animation-duration: 14s;
          animation-direction: reverse;
        }

        .sphere-ring--3 {
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
          .big-panel__content {
            grid-template-columns: 1fr;
            gap: 48px;
          }
          .big-panel__visual {
            order: -1;
            min-height: 220px;
          }
        }

        @media (max-width: 768px) {
          .big-panel__content {
            padding: 32px 24px;
          }
        }
      </style>

      <div class="big-panel" @mousemove=${this.handleMove} @mouseleave=${this.handleLeave}>
        <div class="big-panel__shine"></div>
        <div class="big-panel__content">
          <div class="big-panel__stats">
            <slot></slot>
          </div>
          <div class="big-panel__visual">
            <slot name="visual"></slot>
          </div>
        </div>
      </div>
    `;
  }
}
