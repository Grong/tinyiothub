import { LitElement, html } from "lit";
import { customElement, query } from "lit/decorators.js";
import type * as THREE from "three";

@customElement("point-earth")
export class PointEarth extends LitElement {
  @query("canvas") canvas!: HTMLCanvasElement;

  private THREE: typeof import("three") | null = null;
  private renderer: any = null;
  private scene: any = null;
  private camera: any = null;
  private earth: any = null;
  private atmosphere: any = null;
  private animFrame: number | null = null;
  private mouseX = 0;
  private mouseY = 0;
  private targetX = 0;
  private targetY = 0;
  private isDark = true;
  private initialized = false;

  createRenderRoot() {
    return this;
  }

  connectedCallback() {
    super.connectedCallback();
    this.checkTheme();
    window.addEventListener("resize", this.handleResize);
    window.addEventListener("mousemove", this.handleMouse);
    window.addEventListener("themechange", this.handleThemeChange as EventListener);
    const observer = new MutationObserver(() => this.checkTheme());
    observer.observe(document.documentElement, { attributes: true, attributeFilter: ["data-theme"] });
  }

  disconnectedCallback() {
    super.disconnectedCallback();
    window.removeEventListener("resize", this.handleResize);
    window.removeEventListener("mousemove", this.handleMouse);
    window.removeEventListener("themechange", this.handleThemeChange as EventListener);
    this.dispose();
  }

  private checkTheme() {
    const dark = !document.documentElement.hasAttribute("data-theme") ||
                 document.documentElement.getAttribute("data-theme") !== "light";
    if (dark !== this.isDark) {
      this.isDark = dark;
      this.updateColors();
    }
  }

  private handleThemeChange = () => {
    this.checkTheme();
  };

  private handleResize = () => {
    if (!this.canvas || !this.renderer || !this.camera) return;
    const w = this.canvas.clientWidth || 400;
    const h = this.canvas.clientHeight || 400;
    this.camera.aspect = w / h;
    this.camera.updateProjectionMatrix();
    this.renderer.setSize(w, h, false);
  };

  private handleMouse = (e: MouseEvent) => {
    const rect = this.canvas?.getBoundingClientRect();
    if (!rect) return;
    this.mouseX = ((e.clientX - rect.left) / rect.width - 0.5) * 2;
    this.mouseY = ((e.clientY - rect.top) / rect.height - 0.5) * 2;
  };

  firstUpdated() {
    if (this.initialized) return;
    this.initialized = true;
    // Lazy load Three.js
    import("three").then((mod) => {
      this.THREE = mod;
      this.initThree();
      this.animate();
    });
  }

  private initThree() {
    if (!this.THREE || !this.canvas) return;
    const THREE = this.THREE;
    const w = this.canvas.clientWidth || 400;
    const h = this.canvas.clientHeight || 400;

    // Renderer
    this.renderer = new THREE.WebGLRenderer({
      canvas: this.canvas,
      alpha: true,
      antialias: true,
    });
    this.renderer.setSize(w, h, false);
    this.renderer.setPixelRatio(Math.min(window.devicePixelRatio, 2));

    // Scene
    this.scene = new THREE.Scene();

    // Camera
    this.camera = new THREE.PerspectiveCamera(50, w / h, 0.1, 100);
    this.camera.position.z = 3.2;

    // Lights
    const ambientLight = new THREE.AmbientLight(0xffffff, 0.4);
    this.scene.add(ambientLight);

    const pointLight = new THREE.PointLight(0x00d4ff, 2, 10);
    pointLight.position.set(3, 2, 3);
    this.scene.add(pointLight);

    const pointLight2 = new THREE.PointLight(0x7b61ff, 1.5, 10);
    pointLight2.position.set(-3, -1, 2);
    this.scene.add(pointLight2);

    // Earth
    this.createEarth();
  }

  private createEarth() {
    if (!this.THREE) return;
    const THREE = this.THREE;
    const geometry = new THREE.BufferGeometry();
    const count = 2000;
    const positions = new Float32Array(count * 3);
    const colors = new Float32Array(count * 3);

    // Colors for dark/light theme
    const baseColor = this.isDark
      ? { r: 0.1, g: 0.6, b: 0.8 }
      : { r: 0.1, g: 0.35, b: 0.65 };

    for (let i = 0; i < count; i++) {
      // Random spherical distribution
      const u = Math.random();
      const v = Math.random();
      const phi = 2 * Math.PI * u;
      const theta = Math.acos(2 * v - 1);

      // Slight radius variation
      const r = 0.92 + Math.random() * 0.08;
      const x = r * Math.sin(phi) * Math.cos(theta);
      const y = r * Math.sin(phi) * Math.sin(theta);
      const z = r * Math.cos(phi);

      positions[i * 3] = x;
      positions[i * 3 + 1] = y;
      positions[i * 3 + 2] = z;

      const brightness = 0.5 + Math.random() * 0.5;
      colors[i * 3] = baseColor.r * brightness;
      colors[i * 3 + 1] = baseColor.g * brightness;
      colors[i * 3 + 2] = baseColor.b * brightness;
    }

    geometry.setAttribute("position", new THREE.BufferAttribute(positions, 3));
    geometry.setAttribute("color", new THREE.BufferAttribute(colors, 3));

    const material = new THREE.PointsMaterial({
      size: 0.012,
      vertexColors: true,
      transparent: true,
      opacity: 0.6,
      sizeAttenuation: true,
      depthWrite: true,
    });

    this.earth = new THREE.Points(geometry, material);
    this.scene?.add(this.earth);
  }

  private updateColors() {
    if (!this.earth || !this.THREE) return;

    const baseColor = this.isDark
      ? { r: 0.1, g: 0.6, b: 0.8 }
      : { r: 0.1, g: 0.35, b: 0.65 };

    const colors = this.earth.geometry.attributes.color as any;
    const count = colors.count;

    for (let i = 0; i < count; i++) {
      const brightness = 0.6 + Math.random() * 0.4;
      colors.array[i * 3] = baseColor.r * brightness;
      colors.array[i * 3 + 1] = baseColor.g * brightness;
      colors.array[i * 3 + 2] = baseColor.b * brightness;
    }
    colors.needsUpdate = true;
  }

  private animate = () => {
    this.animFrame = requestAnimationFrame(this.animate);

    // Smooth follow mouse
    this.targetX += (this.mouseX - this.targetX) * 0.02;
    this.targetY += (this.mouseY - this.targetY) * 0.02;

    // Auto rotation + mouse influence
    if (this.earth) {
      this.earth.rotation.y += 0.002;
      this.earth.rotation.x = this.targetY * 0.2;
      this.earth.rotation.z = this.targetX * 0.1;
    }

    this.renderer?.render(this.scene!, this.camera!);
  };

  private dispose() {
    if (this.animFrame !== null) {
      cancelAnimationFrame(this.animFrame);
    }
    this.earth?.geometry.dispose();
    (this.earth?.material as any)?.dispose();
    this.renderer?.dispose();
  }

  render() {
    return html`
      <style>
        :host {
          display: block;
          width: 100%;
          height: 100%;
          min-height: 300px;
        }
        canvas {
          width: 100%;
          height: 100%;
          display: block;
        }
      </style>
      <canvas></canvas>
    `;
  }
}
