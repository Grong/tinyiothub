import { LitElement, html } from "lit";
import { customElement, query } from "lit/decorators.js";
import type * as THREE from "three";

interface SceneData {
  name: string;
  label: string;
  hubColor: { r: number; g: number; b: number };
  accentColor: { r: number; g: number; b: number };
  buildFn: (positions: Float32Array, colors: Float32Array, count: number) => void;
}

@customElement("showcase-viz")
export class ShowcaseViz extends LitElement {
  @query("canvas") canvas!: HTMLCanvasElement;

  private THREE: typeof import("three") | null = null;
  private renderer: any = null;
  private scene: any = null;
  private camera: any = null;
  private points: any = null;
  private geom: any = null;
  private mat: any = null;
  private animFrame: number | null = null;
  private initialized = false;
  private currentScene = 0;
  private morphProgress = 1;
  private holdTime = 6;
  private morphDuration = 3;
  private holdTimer = 0;
  private time = 0;

  private cyan = { r: 0.0, g: 0.83, b: 1.0 };
  private purple = { r: 0.48, g: 0.38, b: 1.0 };
  private gold = { r: 1.0, g: 0.8, b: 0.2 };

  createRenderRoot() {
    return this;
  }

  connectedCallback() {
    super.connectedCallback();
    window.addEventListener("resize", this.handleResize);
  }

  disconnectedCallback() {
    super.disconnectedCallback();
    window.removeEventListener("resize", this.handleResize);
    this.dispose();
  }

  private handleResize = () => {
    if (!this.canvas || !this.renderer || !this.camera) return;
    const w = this.canvas.clientWidth || 400;
    const h = this.canvas.clientHeight || 480;
    this.camera.aspect = w / h;
    this.camera.updateProjectionMatrix();
    this.renderer.setSize(w, h, false);
  };

  firstUpdated() {
    if (this.initialized) return;
    this.initialized = true;
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
    const h = this.canvas.clientHeight || 480;

    this.renderer = new THREE.WebGLRenderer({
      canvas: this.canvas,
      alpha: true,
      antialias: true,
    });
    this.renderer.setSize(w, h, false);
    this.renderer.setPixelRatio(Math.min(window.devicePixelRatio, 2));

    this.scene = new THREE.Scene();

    // Perspective camera looking slightly down
    this.camera = new THREE.PerspectiveCamera(45, w / h, 0.1, 100);
    this.camera.position.set(0, 0.3, 2.8);
    this.camera.lookAt(0, 0, 0);

    // Ambient light
    const ambient = new THREE.AmbientLight(0xffffff, 0.3);
    this.scene.add(ambient);

    // Point light for depth
    const pointLight = new THREE.PointLight(0x00d4ff, 1.5, 10);
    pointLight.position.set(2, 2, 2);
    this.scene.add(pointLight);

    this.createParticles();
  }

  private createParticles() {
    if (!this.THREE) return;
    const THREE = this.THREE;

    const count = 2000;
    const positions = new Float32Array(count * 3);
    const colors = new Float32Array(count * 3);

    // Initialize with first scene
    const scene = this.scenes[this.currentScene];
    this.buildScenePositions(scene, positions, colors, count);

    this.geom = new THREE.BufferGeometry();
    this.geom.setAttribute("position", new THREE.BufferAttribute(positions, 3));
    this.geom.setAttribute("color", new THREE.BufferAttribute(colors, 3));

    this.mat = new THREE.PointsMaterial({
      size: 0.012,
      vertexColors: true,
      transparent: true,
      opacity: 0.85,
      blending: THREE.AdditiveBlending,
      sizeAttenuation: true,
    });

    this.points = new THREE.Points(this.geom, this.mat);
    this.scene.add(this.points);
  }

  private scenes: SceneData[] = [
    {
      name: "neural",
      label: "神经网络",
      hubColor: this.cyan,
      accentColor: this.purple,
      buildFn: (p, c, n) => this.buildNeural(p, c, n),
    },
    {
      name: "orbit",
      label: "轨道系统",
      hubColor: this.purple,
      accentColor: this.cyan,
      buildFn: (p, c, n) => this.buildOrbit(p, c, n),
    },
    {
      name: "flow",
      label: "数据流",
      hubColor: this.gold,
      accentColor: this.cyan,
      buildFn: (p, c, n) => this.buildDataFlow(p, c, n),
    },
    {
      name: "wave",
      label: "波形网络",
      hubColor: this.cyan,
      accentColor: this.gold,
      buildFn: (p, c, n) => this.buildWave(p, c, n),
    },
    {
      name: "cluster",
      label: "边缘集群",
      hubColor: this.purple,
      accentColor: this.gold,
      buildFn: (p, c, n) => this.buildCluster(p, c, n),
    },
  ];

  private buildScenePositions(scene: SceneData, positions: Float32Array, colors: Float32Array, count: number) {
    scene.buildFn(positions, colors, count);
  }

  // 神经网络 - 中心 hub + 辐射连接
  private buildNeural(positions: Float32Array, colors: Float32Array, count: number) {
    const hub = this.cyan;
    const accent = this.purple;
    const nodeCount = 20;
    const scale = 0.9;
    const nodes: { x: number; y: number; z: number }[] = [];

    // Center hub
    for (let i = 0; i < 200 && i < count; i++) {
      const angle = Math.random() * Math.PI * 2;
      const r = Math.random() * 0.25;
      const h = (Math.random() - 0.5) * 0.3;
      positions[i * 3] = Math.cos(angle) * r;
      positions[i * 3 + 1] = h;
      positions[i * 3 + 2] = Math.sin(angle) * r;
      const bright = 0.7 + Math.random() * 0.3;
      colors[i * 3] = hub.r * bright;
      colors[i * 3 + 1] = hub.g * bright;
      colors[i * 3 + 2] = hub.b * bright;
    }

    let idx = 200;

    // Generate satellite nodes
    for (let n = 0; n < nodeCount && idx < count; n++) {
      const theta = (n / nodeCount) * Math.PI * 2 + Math.random() * 0.3;
      const phi = Math.random() * Math.PI;
      const r = (0.9 + Math.random() * 0.6) * scale;
      const x = Math.sin(phi) * Math.cos(theta) * r;
      const y = Math.cos(phi) * r * 0.4 + (Math.random() - 0.5) * 0.25;
      const z = Math.sin(phi) * Math.sin(theta) * r;
      nodes.push({ x, y, z });

      // Node glow
      for (let i = 0; i < 30 && idx < count; i++) {
        const a = Math.random() * Math.PI * 2;
        const p = Math.random() * Math.PI;
        const nr = Math.random() * 0.1;
        positions[idx * 3] = x + Math.sin(p) * Math.cos(a) * nr;
        positions[idx * 3 + 1] = y + Math.sin(p) * Math.sin(a) * nr;
        positions[idx * 3 + 2] = z + Math.cos(p) * nr;
        const bright = 0.6 + Math.random() * 0.35;
        colors[idx * 3] = accent.r * bright;
        colors[idx * 3 + 1] = accent.g * bright;
        colors[idx * 3 + 2] = accent.b * bright;
        idx++;
      }
    }

    // Connection lines
    for (const node of nodes) {
      const steps = 20;
      for (let s = 0; s < steps && idx < count; s++) {
        const t = s / steps;
        const mx = node.x * 0.5;
        const my = node.y * 0.5;
        const mz = node.z * 0.5;
        const u = 1 - t;
        const x = u * u * 0 + 2 * u * t * mx + t * t * node.x;
        const y = u * u * 0 + 2 * u * t * my + t * t * node.y;
        const z = u * u * 0 + 2 * u * t * mz + t * t * node.z;
        positions[idx * 3] = x;
        positions[idx * 3 + 1] = y;
        positions[idx * 3 + 2] = z;
        const bright = 0.25 + t * 0.35;
        colors[idx * 3] = hub.r * bright;
        colors[idx * 3 + 1] = hub.g * bright;
        colors[idx * 3 + 2] = hub.b * bright;
        idx++;
      }
    }

    // Ambient particles
    while (idx < count) {
      const theta = Math.random() * Math.PI * 2;
      const phi = Math.acos(2 * Math.random() - 1);
      const r = 0.4 + Math.random() * 1.8;
      positions[idx * 3] = Math.sin(phi) * Math.cos(theta) * r;
      positions[idx * 3 + 1] = Math.cos(phi) * r * 0.35;
      positions[idx * 3 + 2] = Math.sin(phi) * Math.sin(theta) * r;
      const bright = 0.12 + Math.random() * 0.15;
      colors[idx * 3] = hub.r * bright;
      colors[idx * 3 + 1] = hub.g * bright;
      colors[idx * 3 + 2] = hub.b * bright;
      idx++;
    }
  }

  // 轨道系统 - 中心球体 + 轨道环 + 卫星
  private buildOrbit(positions: Float32Array, colors: Float32Array, count: number) {
    const hub = this.purple;
    const accent = this.cyan;
    const scale = 0.85;

    // Central core
    for (let i = 0; i < 250 && i < count; i++) {
      const theta = Math.random() * Math.PI * 2;
      const phi = Math.acos(2 * Math.random() - 1);
      const r = Math.random() * 0.28;
      positions[i * 3] = Math.sin(phi) * Math.cos(theta) * r;
      positions[i * 3 + 1] = Math.sin(phi) * Math.sin(theta) * r;
      positions[i * 3 + 2] = Math.cos(phi) * r;
      const bright = 0.6 + Math.random() * 0.4;
      colors[i * 3] = hub.r * bright;
      colors[i * 3 + 1] = hub.g * bright;
      colors[i * 3 + 2] = hub.b * bright;
    }

    let idx = 250;

    // Orbital rings
    const rings = [
      { radius: 0.7, tilt: 0.3, count: 100 },
      { radius: 1.0, tilt: 0.6, count: 120 },
      { radius: 1.3, tilt: 1.0, count: 140 },
    ];

    for (const ring of rings) {
      const r = ring.radius * scale;
      for (let i = 0; i < ring.count && idx < count; i++) {
        const theta = (i / ring.count) * Math.PI * 2;
        const x = Math.cos(theta) * r;
        const z = Math.sin(theta) * r;
        const y = z * Math.sin(ring.tilt);
        const z2 = z * Math.cos(ring.tilt);
        positions[idx * 3] = x;
        positions[idx * 3 + 1] = y;
        positions[idx * 3 + 2] = z2;
        const bright = 0.5 + Math.random() * 0.3;
        colors[idx * 3] = accent.r * bright;
        colors[idx * 3 + 1] = accent.g * bright;
        colors[idx * 3 + 2] = accent.b * bright;
        idx++;
      }
    }

    // Satellites
    for (const ring of rings) {
      const r = ring.radius * scale;
      const satCount = 2;
      for (let s = 0; s < satCount && idx < count; s++) {
        const theta = ((s + 0.5) / satCount) * Math.PI * 2;
        const x = Math.cos(theta) * r;
        const z = Math.sin(theta) * r;
        const y = z * Math.sin(ring.tilt);
        const z2 = z * Math.cos(ring.tilt);

        for (let p = 0; p < 25 && idx < count; p++) {
          const a = Math.random() * Math.PI * 2;
          const p2 = Math.random() * Math.PI;
          const nr = Math.random() * 0.07;
          positions[idx * 3] = x + Math.sin(p2) * Math.cos(a) * nr;
          positions[idx * 3 + 1] = y + Math.sin(p2) * Math.sin(a) * nr;
          positions[idx * 3 + 2] = z2 + Math.cos(p2) * nr;
          const bright = 0.8 + Math.random() * 0.2;
          colors[idx * 3] = hub.r * bright;
          colors[idx * 3 + 1] = hub.g * bright;
          colors[idx * 3 + 2] = hub.b * bright;
          idx++;
        }
      }
    }

    // Ambient
    while (idx < count) {
      const theta = Math.random() * Math.PI * 2;
      const phi = Math.acos(2 * Math.random() - 1);
      const r = (0.7 + Math.random() * 1.2) * scale;
      positions[idx * 3] = Math.sin(phi) * Math.cos(theta) * r;
      positions[idx * 3 + 1] = Math.cos(phi) * r * 0.25;
      positions[idx * 3 + 2] = Math.sin(phi) * Math.sin(theta) * r;
      const bright = 0.08 + Math.random() * 0.12;
      colors[idx * 3] = accent.r * bright;
      colors[idx * 3 + 1] = accent.g * bright;
      colors[idx * 3 + 2] = accent.b * bright;
      idx++;
    }
  }

  // 数据流 - 水平流动的粒子带
  private buildDataFlow(positions: Float32Array, colors: Float32Array, count: number) {
    const hub = this.gold;
    const accent = this.cyan;
    const streams = 6;
    const scale = 0.9;

    for (let s = 0; s < streams && s * 300 < count; s++) {
      const y = ((s / streams - 0.5) * 1.2 + (Math.random() - 0.5) * 0.15) * scale;
      const z = (Math.random() - 0.5) * 0.3;
      const baseBright = 0.4 + Math.random() * 0.3;

      // Stream particles
      for (let i = 0; i < 250 && s * 300 + i < count; i++) {
        const x = (i / 250 - 0.5) * 3.2 * scale;
        const wave = Math.sin(x * 2 + s) * 0.12;
        positions[(s * 300 + i) * 3] = x + (Math.random() - 0.5) * 0.08;
        positions[(s * 300 + i) * 3 + 1] = y + wave + (Math.random() - 0.5) * 0.06;
        positions[(s * 300 + i) * 3 + 2] = z + (Math.random() - 0.5) * 0.08;

        const bright = baseBright + Math.abs(Math.sin(x * 3)) * 0.4;
        const col = i % 3 === 0 ? hub : accent;
        colors[(s * 300 + i) * 3] = col.r * bright;
        colors[(s * 300 + i) * 3 + 1] = col.g * bright;
        colors[(s * 300 + i) * 3 + 2] = col.b * bright;
      }
    }

    // Add flowing connection nodes at intersections
    let idx = streams * 250;
    const nodeCount = 8;
    for (let n = 0; n < nodeCount && idx < count; n++) {
      const x = ((n % 4) - 1.5) * 0.8 * scale;
      const y = (Math.floor(n / 4) - 0.5) * 0.4 * scale;
      const z = (Math.random() - 0.5) * 0.2;

      for (let p = 0; p < 35 && idx < count; p++) {
        const a = Math.random() * Math.PI * 2;
        const p2 = Math.random() * Math.PI;
        const r = Math.random() * 0.08;
        positions[idx * 3] = x + Math.sin(p2) * Math.cos(a) * r;
        positions[idx * 3 + 1] = y + Math.sin(p2) * Math.sin(a) * r;
        positions[idx * 3 + 2] = z + Math.cos(p2) * r;
        const bright = 0.7 + Math.random() * 0.3;
        colors[idx * 3] = hub.r * bright;
        colors[idx * 3 + 1] = hub.g * bright;
        colors[idx * 3 + 2] = hub.b * bright;
        idx++;
      }
    }

    // Ambient particles
    while (idx < count) {
      positions[idx * 3] = (Math.random() - 0.5) * 3.2 * scale;
      positions[idx * 3 + 1] = (Math.random() - 0.5) * 1.5;
      positions[idx * 3 + 2] = (Math.random() - 0.5) * 1.5;
      const bright = 0.1 + Math.random() * 0.15;
      colors[idx * 3] = accent.r * bright;
      colors[idx * 3 + 1] = accent.g * bright;
      colors[idx * 3 + 2] = accent.b * bright;
      idx++;
    }
  }

  // 波形网络 - 起伏的波形 + 节点
  private buildWave(positions: Float32Array, colors: Float32Array, count: number) {
    const hub = this.cyan;
    const accent = this.gold;
    const scale = 0.85;

    // Grid of wave points
    const gridSize = 16;
    const spacing = 0.18 * scale;
    let idx = 0;

    for (let xi = 0; xi < gridSize && idx < count; xi++) {
      for (let zi = 0; zi < gridSize && idx < count; zi++) {
        const baseX = (xi - gridSize / 2) * spacing;
        const baseZ = (zi - gridSize / 2) * spacing;
        const dist = Math.sqrt(baseX * baseX + baseZ * baseZ);
        const wave = Math.sin(dist * 3) * 0.18 + Math.sin(baseX * 2) * 0.08;
        const x = baseX + (Math.random() - 0.5) * spacing * 0.4;
        const y = wave + (Math.random() - 0.5) * 0.04;
        const z = baseZ + (Math.random() - 0.5) * spacing * 0.4;

        positions[idx * 3] = x;
        positions[idx * 3 + 1] = y;
        positions[idx * 3 + 2] = z;

        const bright = 0.3 + Math.abs(Math.sin(dist * 2)) * 0.35;
        colors[idx * 3] = hub.r * bright;
        colors[idx * 3 + 1] = hub.g * bright;
        colors[idx * 3 + 2] = hub.b * bright;
        idx++;
      }
    }

    // Peak nodes
    const peaks = [
      { x: 0, z: 0 },
      { x: 0.4, z: 0.25 },
      { x: -0.3, z: 0.4 },
      { x: 0.25, z: -0.3 },
      { x: -0.25, z: -0.25 },
    ];

    for (const peak of peaks) {
      for (let p = 0; p < 40 && idx < count; p++) {
        const a = Math.random() * Math.PI * 2;
        const p2 = Math.random() * Math.PI;
        const r = Math.random() * 0.1;
        positions[idx * 3] = peak.x * scale + Math.sin(p2) * Math.cos(a) * r;
        positions[idx * 3 + 1] = 0.25 + Math.sin(p2) * Math.sin(a) * r;
        positions[idx * 3 + 2] = peak.z * scale + Math.cos(p2) * r;
        const bright = 0.7 + Math.random() * 0.3;
        colors[idx * 3] = accent.r * bright;
        colors[idx * 3 + 1] = accent.g * bright;
        colors[idx * 3 + 2] = accent.b * bright;
        idx++;
      }
    }

    // Ambient
    while (idx < count) {
      positions[idx * 3] = (Math.random() - 0.5) * 2.5;
      positions[idx * 3 + 1] = (Math.random() - 0.5) * 0.8;
      positions[idx * 3 + 2] = (Math.random() - 0.5) * 2.5;
      const bright = 0.08 + Math.random() * 0.12;
      colors[idx * 3] = hub.r * bright;
      colors[idx * 3 + 1] = hub.g * bright;
      colors[idx * 3 + 2] = hub.b * bright;
      idx++;
    }
  }

  // 边缘集群 - 分散的设备节点 + 云中心
  private buildCluster(positions: Float32Array, colors: Float32Array, count: number) {
    const hub = this.purple;
    const accent = this.gold;
    const scale = 0.85;

    // Central cloud
    for (let i = 0; i < 350 && i < count; i++) {
      const theta = Math.random() * Math.PI * 2;
      const phi = Math.acos(2 * Math.random() - 1);
      const r = Math.random() * 0.4 * scale;
      const x = Math.sin(phi) * Math.cos(theta) * r;
      const y = Math.cos(phi) * r * 0.35;
      const z = Math.sin(phi) * Math.sin(theta) * r;
      positions[i * 3] = x;
      positions[i * 3 + 1] = y;
      positions[i * 3 + 2] = z;
      const bright = 0.5 + Math.random() * 0.4;
      colors[i * 3] = hub.r * bright;
      colors[i * 3 + 1] = hub.g * bright;
      colors[i * 3 + 2] = hub.b * bright;
    }

    let idx = 350;

    // Edge devices - scattered around
    const deviceCount = 12;
    for (let d = 0; d < deviceCount && idx < count; d++) {
      const angle = (d / deviceCount) * Math.PI * 2 + Math.random() * 0.4;
      const r = (0.9 + Math.random() * 0.7) * scale;
      const dx = Math.cos(angle) * r;
      const dy = (Math.random() - 0.5) * 0.6;
      const dz = Math.sin(angle) * r;

      // Device shape (cube-ish)
      for (let p = 0; p < 60 && idx < count; p++) {
        const shape = p % 3;
        let px = dx, py = dy, pz = dz;

        if (shape === 0) {
          px = dx + (Math.random() - 0.5) * 0.12;
          py = dy + (Math.random() - 0.5) * 0.12;
          pz = dz + (Math.random() - 0.5) * 0.12;
        } else if (shape === 1) {
          const a = Math.random() * Math.PI * 2;
          const h = Math.random() * 0.12;
          px = dx + Math.cos(a) * 0.07;
          py = dy + h - 0.06;
          pz = dz + Math.sin(a) * 0.07;
        } else {
          const a = Math.random() * Math.PI * 2;
          const p2 = Math.random() * Math.PI;
          const rad = 0.06;
          px = dx + Math.sin(p2) * Math.cos(a) * rad;
          py = dy + Math.sin(p2) * Math.sin(a) * rad;
          pz = dz + Math.cos(p2) * rad;
        }

        positions[idx * 3] = px;
        positions[idx * 3 + 1] = py;
        positions[idx * 3 + 2] = pz;
        const bright = 0.5 + Math.random() * 0.4;
        colors[idx * 3] = accent.r * bright;
        colors[idx * 3 + 1] = accent.g * bright;
        colors[idx * 3 + 2] = accent.b * bright;
        idx++;
      }
    }

    // Connection streams to cloud
    for (let i = 0; i < 200 && idx < count; i++) {
      const t = Math.random();
      const angle = Math.random() * Math.PI * 2;
      const r = t * 0.9 * scale;
      positions[idx * 3] = Math.cos(angle) * r;
      positions[idx * 3 + 1] = (Math.random() - 0.5) * 0.3;
      positions[idx * 3 + 2] = Math.sin(angle) * r;
      const bright = 0.2 + t * 0.3;
      colors[idx * 3] = hub.r * bright;
      colors[idx * 3 + 1] = hub.g * bright;
      colors[idx * 3 + 2] = hub.b * bright;
      idx++;
    }

    // Ambient
    while (idx < count) {
      positions[idx * 3] = (Math.random() - 0.5) * 2.5;
      positions[idx * 3 + 1] = (Math.random() - 0.5) * 1.2;
      positions[idx * 3 + 2] = (Math.random() - 0.5) * 2.5;
      const bright = 0.08 + Math.random() * 0.12;
      colors[idx * 3] = accent.r * bright;
      colors[idx * 3 + 1] = accent.g * bright;
      colors[idx * 3 + 2] = accent.b * bright;
      idx++;
    }
  }

  private interpolate(a: number, b: number, t: number): number {
    return a + (b - a) * t;
 }

  private easeInOutCubic(t: number): number {
    return t < 0.5 ? 4 * t * t * t : 1 - Math.pow(-2 * t + 2, 3) / 2;
  }

  private animate = () => {
    this.animFrame = requestAnimationFrame(this.animate);
    this.time += 0.016;

    if (this.morphProgress >= 1) {
      // Holding
      this.holdTimer += 0.016;
      if (this.holdTimer >= this.holdTime) {
        this.morphProgress = 0;
        this.holdTimer = 0;
      }
    } else {
      // Morphing
      this.morphProgress += 0.016 / this.morphDuration;

      const fromScene = this.scenes[this.currentScene];
      const toScene = this.scenes[(this.currentScene + 1) % this.scenes.length];

      const count = 2000;
      const fromPos = new Float32Array(count * 3);
      const fromCol = new Float32Array(count * 3);
      const toPos = new Float32Array(count * 3);
      const toCol = new Float32Array(count * 3);

      fromScene.buildFn(fromPos, fromCol, count);
      toScene.buildFn(toPos, toCol, count);

      const eased = this.easeInOutCubic(Math.min(this.morphProgress, 1));
      const posAttr = this.geom.attributes.position;
      const colAttr = this.geom.attributes.color;

      for (let i = 0; i < count * 3; i++) {
        posAttr.array[i] = this.interpolate(fromPos[i], toPos[i], eased);
        colAttr.array[i] = this.interpolate(fromCol[i], toCol[i], eased);
      }

      posAttr.needsUpdate = true;
      colAttr.needsUpdate = true;

      if (this.morphProgress >= 1) {
        this.currentScene = (this.currentScene + 1) % this.scenes.length;
      }
    }

    // Gentle rotation for depth
    if (this.points) {
      this.points.rotation.y += 0.001;
    }

    this.renderer?.render(this.scene!, this.camera!);
  };

  private dispose() {
    if (this.animFrame !== null) {
      cancelAnimationFrame(this.animFrame);
    }
    this.geom?.dispose();
    this.mat?.dispose();
    this.renderer?.dispose();
  }

  render() {
    return html`
      <style>
        :host {
          display: block;
          width: 100%;
          height: 100%;
          min-height: 480px;
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
