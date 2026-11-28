/**
 * 「AI 大脑」语音球：Canvas 2D 流体球 + 环形波形 + HUD 刻度。
 *
 * 从 view-story-map 抽出来的独立渲染器：只依赖 canvas 和主题属性，
 * 不读页面状态；可见性/尺寸/DOM 生命周期由调用方负责 dispose。
 */

interface OrbTheme {
  blobs: string[];
  ring: string;
  hud: string;
}

const DARK: OrbTheme = {
  blobs: ['#7ff0ff', '#6f8bff', '#a58fff', '#7fe0d8', '#c78fff'],
  ring: '#a8ecff',
  hud: 'rgba(150,200,255,0.6)',
};

const LIGHT: OrbTheme = {
  blobs: ['#5f9fc0', '#6f7cc0', '#9a7fbf', '#5f9aa8', '#b98fb0'],
  ring: '#5f7fa8',
  hud: 'rgba(50,70,100,0.45)',
};

const BLOBS = 16;

function makeBlobs() {
  let seed = 20261127;
  const rnd = () => {
    seed = (seed * 16807) % 2147483647;
    return seed / 2147483647;
  };
  return Array.from({ length: BLOBS }, (_, index) => ({
    ang: (index / BLOBS) * Math.PI * 2 + rnd() * 0.5,
    rad: 0.1 + rnd() * 0.36,
    size: 0.38 + rnd() * 0.4,
    speed: (rnd() < 0.5 ? -1 : 1) * (0.05 + rnd() * 0.12),
    color: index % 5,
    phase: rnd() * Math.PI * 2,
  }));
}

export class MapOrb {
  private canvas: HTMLCanvasElement;
  private ctx: CanvasRenderingContext2D;
  private reduce = matchMedia('(prefers-reduced-motion: reduce)').matches;
  private blobs = makeBlobs();
  private width = 0;
  private height = 0;
  private dpr = 1;
  private energy = 0.3;
  private boost = 0;
  private pulse = 0;
  private t0 = 0;
  private raf = 0;

  constructor(canvas: HTMLCanvasElement) {
    this.canvas = canvas;
    const ctx = canvas.getContext('2d');
    if (!ctx) throw new Error('2D canvas unavailable');
    this.ctx = ctx;
    this.resize();
    this.raf = requestAnimationFrame(this.frame);
    window.addEventListener('resize', this.resize);
    document.addEventListener('visibilitychange', this.onVisibility);
  }

  /** 新内容/交互时给球体一次能量脉冲。 */
  ping(): void {
    this.boost = 1;
    this.pulse = 1;
  }

  dispose(): void {
    cancelAnimationFrame(this.raf);
    this.raf = 0;
    window.removeEventListener('resize', this.resize);
    document.removeEventListener('visibilitychange', this.onVisibility);
  }

  private resize = (): void => {
    this.dpr = Math.min(2, window.devicePixelRatio || 1);
    this.width = this.canvas.clientWidth;
    this.height = this.canvas.clientHeight;
    this.canvas.width = Math.round(this.width * this.dpr);
    this.canvas.height = Math.round(this.height * this.dpr);
    this.ctx.setTransform(this.dpr, 0, 0, this.dpr, 0, 0);
  };

  private onVisibility = (): void => {
    if (document.hidden) {
      cancelAnimationFrame(this.raf);
      this.raf = 0;
    } else if (!this.reduce && !this.raf) {
      this.raf = requestAnimationFrame(this.frame);
    }
  };

  private hexA(hex: string, alpha: number): string {
    const value = parseInt(hex.slice(1), 16);
    const a = Math.max(0, Math.min(1, alpha));
    return `rgba(${(value >> 16) & 255}, ${(value >> 8) & 255}, ${value & 255}, ${a})`;
  }

  private frame = (now: number): void => {
    if (!this.t0) this.t0 = now;
    const time = (now - this.t0) / 1000;
    const theme = document.documentElement.dataset.theme === 'light' ? LIGHT : DARK;
    const isDark = theme === DARK;
    const cx = 165;
    const cy = this.height * 0.44;
    const R = 104;
    const ctx = this.ctx;

    this.boost *= 0.972;
    const target = 0.3 + this.boost * 0.7 + (this.reduce ? 0 : 0.04 * Math.sin(time * 0.7));
    this.energy += (target - this.energy) * 0.06;

    ctx.setTransform(this.dpr, 0, 0, this.dpr, 0, 0);
    ctx.clearRect(0, 0, this.width, this.height);

    const halo = ctx.createRadialGradient(
      cx,
      cy,
      R * 0.3,
      cx,
      cy,
      Math.max(this.width * 0.7, this.height * 0.6),
    );
    halo.addColorStop(0, isDark ? 'rgba(70, 120, 210, 0.16)' : 'rgba(120, 150, 190, 0.1)');
    halo.addColorStop(0.4, isDark ? 'rgba(90, 70, 190, 0.05)' : 'rgba(150, 150, 190, 0.04)');
    halo.addColorStop(1, 'rgba(120, 140, 180, 0)');
    ctx.fillStyle = halo;
    ctx.fillRect(0, 0, this.width, this.height);

    ctx.save();
    ctx.beginPath();
    ctx.arc(cx, cy, R, 0, 6.283);
    ctx.clip();
    ctx.globalCompositeOperation = isDark ? 'screen' : 'source-over';
    for (const blob of this.blobs) {
      const angle = blob.ang + (this.reduce ? 0 : time * blob.speed);
      const wob = this.reduce ? 0 : Math.sin(time * 0.9 + blob.phase) * 0.07;
      const x = cx + Math.cos(angle) * blob.rad * R * (1 + wob);
      const y = cy + Math.sin(angle) * blob.rad * R * 0.9 * (1 + wob);
      const radius = blob.size * R * (0.9 + this.energy * 0.3);
      const gradient = ctx.createRadialGradient(x, y, 0, x, y, radius);
      gradient.addColorStop(0, this.hexA(theme.blobs[blob.color], isDark ? 0.42 : 0.2));
      gradient.addColorStop(0.5, this.hexA(theme.blobs[blob.color], isDark ? 0.12 : 0.06));
      gradient.addColorStop(1, this.hexA(theme.blobs[blob.color], 0));
      ctx.fillStyle = gradient;
      ctx.beginPath();
      ctx.arc(x, y, radius, 0, 6.283);
      ctx.fill();
    }
    ctx.restore();

    const core = ctx.createRadialGradient(cx, cy, 0, cx, cy, R * 0.4);
    core.addColorStop(0, isDark ? 'rgba(255,255,255,0.9)' : 'rgba(255,255,255,0.85)');
    core.addColorStop(0.4, isDark ? 'rgba(210,245,255,0.35)' : 'rgba(180,205,230,0.25)');
    core.addColorStop(1, 'rgba(160,210,255,0)');
    ctx.fillStyle = core;
    ctx.beginPath();
    ctx.arc(cx, cy, R * 0.4, 0, 6.283);
    ctx.fill();

    const rim = ctx.createLinearGradient(cx - R, cy - R, cx + R, cy + R);
    rim.addColorStop(0, isDark ? 'rgba(225,248,255,0.85)' : 'rgba(70,90,135,0.55)');
    rim.addColorStop(0.45, isDark ? 'rgba(140,200,255,0.28)' : 'rgba(90,110,155,0.2)');
    rim.addColorStop(1, 'rgba(120,140,255,0.05)');
    ctx.strokeStyle = rim;
    ctx.lineWidth = 1.3;
    ctx.beginPath();
    ctx.arc(cx, cy, R - 0.5, 0, 6.283);
    ctx.stroke();

    const bars = 150;
    const r0 = R * 1.14;
    for (let index = 0; index < bars; index += 1) {
      const angle = (index / bars) * Math.PI * 2 - Math.PI / 2;
      const wave =
        0.5 +
        0.28 * Math.sin(time * 4.2 + index * 0.55) +
        0.22 * Math.sin(time * 7.1 + index * 1.31) +
        0.18 * Math.sin(time * 2.3 - index * 0.87);
      const level = Math.abs(wave) * (0.32 + this.energy * 0.9);
      const len = 3 + level * R * 0.3;
      ctx.strokeStyle = this.hexA(theme.ring, (isDark ? 0.72 : 0.55) * (0.45 + level * 0.55));
      ctx.lineWidth = 1.5;
      ctx.beginPath();
      ctx.moveTo(cx + Math.cos(angle) * r0, cy + Math.sin(angle) * r0);
      ctx.lineTo(cx + Math.cos(angle) * (r0 + len), cy + Math.sin(angle) * (r0 + len));
      ctx.stroke();
    }

    const tickR = R * 1.6;
    for (let index = 0; index < 72; index += 1) {
      const angle = (index / 72) * Math.PI * 2 - Math.PI / 2;
      const major = index % 6 === 0;
      const len = major ? 6 : 2.5;
      ctx.strokeStyle = this.hexA(theme.hud, major ? (isDark ? 0.7 : 0.55) : (isDark ? 0.3 : 0.24));
      ctx.lineWidth = major ? 1.2 : 0.8;
      ctx.beginPath();
      ctx.moveTo(cx + Math.cos(angle) * tickR, cy + Math.sin(angle) * tickR);
      ctx.lineTo(cx + Math.cos(angle) * (tickR + len), cy + Math.sin(angle) * (tickR + len));
      ctx.stroke();
    }

    const arcR = R * 1.78;
    const start = this.reduce ? 0 : time * 0.5;
    ctx.strokeStyle = isDark ? 'rgba(170,230,255,0.5)' : 'rgba(60,90,130,0.35)';
    ctx.lineWidth = 1.4;
    ctx.beginPath();
    ctx.arc(cx, cy, arcR, start, start + 0.55);
    ctx.stroke();

    if (this.pulse > 0.02) {
      const radius = R * 1.15 + (1 - this.pulse) * R * 1.5;
      ctx.strokeStyle = isDark
        ? `rgba(170,220,255,${this.pulse * 0.28})`
        : `rgba(90,110,150,${this.pulse * 0.18})`;
      ctx.lineWidth = 1;
      ctx.beginPath();
      ctx.arc(cx, cy, radius, 0, 6.283);
      ctx.stroke();
    }

    this.pulse *= 0.975;
    if (!this.reduce) this.raf = requestAnimationFrame(this.frame);
  };
}
