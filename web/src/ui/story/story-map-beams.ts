/**
 * 大脑 → 技能行的连线。固定 SVG 覆盖层，按卡片实际位置画贝塞尔曲线。
 *
 * rAF 节流 + 路径指纹：滚动/resize/主题切换只在真的变化时写 DOM。
 */

const ORB_CENTER_X = 165 + 30;

export class MapBeams {
  private raf = 0;
  private key = '';

  constructor(
    private shell: HTMLElement,
    private svg: SVGSVGElement,
  ) {}

  schedule(): void {
    if (this.raf) return;
    this.raf = requestAnimationFrame(() => {
      this.raf = 0;
      this.draw();
    });
  }

  dispose(): void {
    cancelAnimationFrame(this.raf);
    this.raf = 0;
  }

  private draw(): void {
    const shellRect = this.shell.getBoundingClientRect();
    const pitch = document.documentElement.dataset.theme === 'light' ? 'rgba(17,24,39,' : 'rgba(255,255,255,';
    const cards = [...this.shell.querySelectorAll<HTMLElement>('.map-lane .map-card--skill')];
    const paths = cards.map((card) => {
      const rect = card.getBoundingClientRect();
      const cx = ORB_CENTER_X;
      const cy = shellRect.height * 0.44;
      const x2 = rect.left - shellRect.left;
      const y2 = rect.top - shellRect.top + rect.height * 0.42;
      const dx = Math.max(60, (x2 - cx) * 0.55);
      return `M ${cx} ${cy} C ${cx + dx} ${cy}, ${x2 - dx} ${y2}, ${x2} ${y2}`;
    });
    const key = `${pitch}|${paths.join('|')}`;
    if (key === this.key) return;
    this.key = key;
    this.svg.innerHTML = paths
      .map((d) => `<path d="${d}" fill="none" stroke="${pitch}0.18)" stroke-width="1.1"></path>`)
      .join('');
  }
}
