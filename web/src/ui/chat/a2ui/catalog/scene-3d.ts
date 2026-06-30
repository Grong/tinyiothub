import { LitElement, html, css, nothing, type TemplateResult } from "lit";
import { property } from "lit/decorators/property.js";
import * as THREE from "three";
import { OrbitControls } from "three/examples/jsm/controls/OrbitControls.js";
import { GLTFLoader, type GLTF } from "three/examples/jsm/loaders/GLTFLoader.js";

// ── Status colors (from CSS variables, fallback for SSR) ──
function getStatusColors(): Record<string, string> {
  if (typeof document === "undefined") return {
    online: "#22c55e",
    offline: "#71717a",
    warning: "#f59e0b",
    error: "#ef4444",
  };
  const styles = getComputedStyle(document.documentElement);
  return {
    online: styles.getPropertyValue("--ok").trim() || "#22c55e",
    offline: styles.getPropertyValue("--muted").trim() || "#71717a",
    warning: styles.getPropertyValue("--warn").trim() || "#f59e0b",
    error: styles.getPropertyValue("--danger").trim() || "#ef4444",
  };
}

// ── Device instance from scene metadata ──
type DeviceInstance = {
  instanceId: string;
  deviceId: string;
  position: [number, number, number];
  floorId?: string;
};

// ── Floor info from scene metadata ──
type FloorInfo = {
  id: string;
  name: string;
  level: number;
  yOffset: number;
  outline?: number[][];
};

// ── Scene metadata type ──
type SceneMetadata = {
  floors?: FloorInfo[];
  defaultCamera?: { position: number[]; target: number[] };
  deviceInstances?: DeviceInstance[];
};

/**
 * A2UI Scene3D — LitElement wrapping Three.js for 3D building visualization.
 *
 * Lifecycle: connectedCallback → initThreeJS → load model → render markers
 *            updated → refresh markers/data
 *            disconnectedCallback → dispose Three.js resources
 */
export class A2uiScene3D extends LitElement {
  static styles = css`
    :host {
      display: block;
      position: relative;
      width: 100%;
      height: 400px;
      border-radius: 8px;
      overflow: hidden;
      background: var(--bg);
    }
    .scene3d-canvas {
      width: 100%;
      height: 100%;
      display: block;
    }
    .scene3d-overlay {
      position: absolute;
      top: 0;
      left: 0;
      width: 100%;
      height: 100%;
      pointer-events: none;
    }
    .scene3d-marker {
      position: absolute;
      transform: translate(-50%, -100%);
      pointer-events: auto;
      cursor: pointer;
      display: flex;
      flex-direction: column;
      align-items: center;
      gap: 2px;
    }
    .scene3d-marker__dot {
      width: 12px;
      height: 12px;
      border-radius: 50%;
      border: 2px solid rgba(255,255,255,0.8);
      box-shadow: 0 0 8px currentColor;
    }
    .scene3d-marker__label {
      font-size: 10px;
      color: white;
      background: rgba(0,0,0,0.6);
      padding: 1px 6px;
      border-radius: 4px;
      white-space: nowrap;
      text-shadow: 0 1px 2px rgba(0,0,0,0.8);
    }
    .scene3d-floorbar {
      position: absolute;
      top: 12px;
      left: 12px;
      display: flex;
      flex-direction: column;
      gap: 4px;
      pointer-events: auto;
    }
    .scene3d-floor-btn {
      padding: 4px 12px;
      border-radius: 4px;
      background: rgba(0,0,0,0.5);
      color: #fff;
      border: none;
      cursor: pointer;
      font-size: 12px;
      transition: background 0.15s;
    }
    .scene3d-floor-btn:hover { background: rgba(0,0,0,0.7); }
    .scene3d-floor-btn--active { background: rgba(0,212,170,0.8); }
    .scene3d-loading {
      position: absolute;
      inset: 0;
      display: flex;
      flex-direction: column;
      align-items: center;
      justify-content: center;
      gap: 10px;
      color: var(--muted, #888);
      background: rgba(0, 0, 0, 0.55);
      backdrop-filter: blur(8px);
      animation: scene3d-fade-in 0.3s ease-out;
      z-index: 5;
    }
    .scene3d-loading__spinner {
      width: 40px;
      height: 40px;
      border: 3px solid color-mix(in srgb, var(--muted, #888) 20%, transparent);
      border-top-color: var(--accent, #0098FF);
      border-right-color: var(--accent-gradient-end, #0098FF);
      border-radius: 50%;
      animation: scene3d-spin 0.7s linear infinite;
      box-shadow: 0 0 16px color-mix(in srgb, var(--accent, #0098FF) 20%, transparent);
    }
    .scene3d-loading__title {
      font-size: 15px;
      font-weight: 600;
      color: var(--text, #e4e4e7);
    }
    .scene3d-loading__progress-bar {
      width: 260px;
      height: 4px;
      background: color-mix(in srgb, var(--muted, #888) 20%, transparent);
      border-radius: 2px;
      overflow: hidden;
    }
    .scene3d-loading__progress-bar--indeterminate {
      /* subtle pulse */
    }
    .scene3d-loading__progress-fill {
      height: 100%;
      background: linear-gradient(90deg, #00d4ff, #0098ff, #7b61ff);
      background-size: 200% 100%;
      border-radius: 2px;
      transition: width 0.3s ease-out;
      animation: scene3d-shimmer 2s ease-in-out infinite;
    }
    .scene3d-loading__progress-fill--indeterminate {
      width: 30% !important;
      animation: scene3d-indeterminate 1.5s ease-in-out infinite;
      border-radius: 2px;
    }
    .scene3d-loading__progress-text {
      font-size: 18px;
      font-weight: 700;
      color: var(--text, #e4e4e7);
    }
    .scene3d-loading__bytes {
      font-size: 12px;
      opacity: 0.6;
    }
    .scene3d-loading__hint {
      display: flex;
      align-items: center;
      gap: 6px;
      margin-top: 4px;
      padding: 8px 16px;
      background: var(--warn-subtle, rgba(245,158,11,0.12));
      border-radius: 8px;
      font-size: 12px;
      color: var(--warn, #f59e0b);
      max-width: 320px;
      text-align: center;
      animation: scene3d-fade-in 0.3s ease-out;
    }
    @keyframes scene3d-spin {
      to { transform: rotate(360deg); }
    }
    @keyframes scene3d-indeterminate {
      0%   { transform: translateX(-100%); }
      100% { transform: translateX(400%); }
    }
    @keyframes scene3d-fade-in {
      from { opacity: 0; transform: translateY(4px); }
      to   { opacity: 1; transform: translateY(0); }
    }
    @keyframes scene3d-shimmer {
      0%   { background-position: 200% 0; }
      100% { background-position: -200% 0; }
    }
    .scene3d-error {
      position: absolute;
      inset: 0;
      display: flex;
      flex-direction: column;
      align-items: center;
      justify-content: center;
      gap: 12px;
      color: #ef4444;
      font-size: 14px;
      padding: 20px;
      text-align: center;
    }
    .scene3d-error button {
      padding: 6px 16px;
      border-radius: 6px;
      border: 1px solid rgba(255,255,255,0.1);
      background: rgba(255,255,255,0.05);
      color: var(--text, #fff);
      cursor: pointer;
      font-size: 13px;
    }
  `;

  @property({ type: Object }) dataModel: Record<string, unknown> = {};
  @property({ type: Object }) onAction?: (fn: string, args: Record<string, unknown>) => void;

  // Three.js internals
  private renderer?: THREE.WebGLRenderer;
  private scene?: THREE.Scene;
  private camera?: THREE.PerspectiveCamera;
  private controls?: OrbitControls;
  private modelGroup?: THREE.Group;
  private rafId?: number;
  private resizeObserver?: ResizeObserver;
  private markers: Array<{ element: HTMLElement; worldPos: THREE.Vector3; floorId?: string; deviceId: string }> = [];
  private overlayEl?: HTMLElement;
  private floors: FloorInfo[] = [];
  private deviceInstances: DeviceInstance[] = [];
  private activeFloorId?: string;
  private loadState: "idle" | "loading" | "error" | "loaded" = "idle";
  private errorMsg = "";
  private loadProgress = 0;       // 0-100
  private loadedBytes = 0;
  private totalBytes = 0;
  private loadSlow = false;        // true after 3s of loading
  private _slowTimer: ReturnType<typeof setTimeout> | null = null;

  // ── Lit lifecycle ──

  connectedCallback() {
    super.connectedCallback();
    // Immediately show loading state so users see feedback right away
    this.loadState = "loading";
    this.requestUpdate();
    // Delay Three.js init until first render completes
    requestAnimationFrame(() => this.initScene());
  }

  disconnectedCallback() {
    super.disconnectedCallback();
    this.dispose();
  }

  updated(changed: Map<string, unknown>) {
    super.updated(changed);
    if (changed.has("dataModel")) {
      this.onDataModelChanged();
    }
  }

  // ── Scene init ──

  private async initScene() {
    if (this.renderer) return; // Already initialized

    const canvas = this.shadowRoot?.querySelector(".scene3d-canvas") as HTMLCanvasElement | null;
    const overlay = this.shadowRoot?.querySelector(".scene3d-overlay") as HTMLElement | null;
    if (!canvas || !overlay) return;
    this.overlayEl = overlay;

    const rect = this.getBoundingClientRect();
    if (rect.width === 0 || rect.height === 0) return;

    // Renderer — alpha so CSS background shows through
    this.renderer = new THREE.WebGLRenderer({ canvas, antialias: true, alpha: true });
    this.renderer.setSize(rect.width, rect.height);
    this.renderer.setPixelRatio(Math.min(window.devicePixelRatio, 2));
    this.renderer.setClearColor(0x000000, 0);
    this.renderer.toneMapping = THREE.ACESFilmicToneMapping;
    this.renderer.toneMappingExposure = 1.5;
    this.renderer.localClippingEnabled = true;

    // Scene
    this.scene = new THREE.Scene();
    // Ambient — base illumination so nothing is pitch black
    this.scene.add(new THREE.AmbientLight(0xffffff, 4.0));
    // Hemisphere — sky/ground gradient
    const hemiLight = new THREE.HemisphereLight(0xddeeff, 0x8899aa, 3.0);
    this.scene.add(hemiLight);
    // Key light — strong main light
    const keyLight = new THREE.DirectionalLight(0xffffff, 8.0);
    keyLight.position.set(10, 20, 10);
    this.scene.add(keyLight);
    // Fill light — opposite side to reduce shadow
    const fillLight = new THREE.DirectionalLight(0xffffff, 4.0);
    fillLight.position.set(-10, 5, -10);
    this.scene.add(fillLight);
    // Rim light — highlights edges against dark background
    const rimLight = new THREE.DirectionalLight(0xffffff, 3.0);
    rimLight.position.set(0, 5, -20);
    this.scene.add(rimLight);

    // Camera
    this.camera = new THREE.PerspectiveCamera(45, rect.width / rect.height, 0.1, 1000);
    this.camera.position.set(20, 20, 20);

    // Controls
    this.controls = new OrbitControls(this.camera, canvas);
    this.controls.enableDamping = true;
    this.controls.dampingFactor = 0.05;

    // Load model
    await this.loadModel();

    // Start render loop
    this._animate();

    // Resize observer
    this.resizeObserver = new ResizeObserver((entries) => {
      for (const entry of entries) {
        const { width, height } = entry.contentRect;
        if (this.camera && this.renderer) {
          this.camera.aspect = width / height;
          this.camera.updateProjectionMatrix();
          this.renderer.setSize(width, height);
        }
      }
    });
    this.resizeObserver.observe(this);
  }

  private async loadModel() {
    let modelUrl = String(this.dataModel.modelUrl || "");

    // Resolve resourceId to modelUrl if modelUrl is not provided
    if (!modelUrl) {
      const resourceId = String(this.dataModel.resourceId || "");
      if (resourceId) {
        modelUrl = await this.resolveResourceUrl(resourceId);
      }
    }

    if (!modelUrl) {
      this.loadState = "error";
      this.errorMsg = "Missing modelUrl";
      this.requestUpdate();
      return;
    }

    this.loadState = "loading";
    this.loadProgress = 0;
    this.loadedBytes = 0;
    this.totalBytes = 0;
    this.loadSlow = false;
    this.requestUpdate();

    // Show "slow connection" hint after 3 seconds
    this._slowTimer = setTimeout(() => {
      this.loadSlow = true;
      this.requestUpdate();
    }, 3000);

    const glbUrl = modelUrl;

    const loader = new GLTFLoader();
    try {
      const gltf = await new Promise<GLTF>((resolve, reject) => {
        loader.load(
          glbUrl,
          resolve,
          (xhr) => {
            if (xhr.total > 0) {
              this.loadProgress = Math.round((xhr.loaded / xhr.total) * 100);
              this.loadedBytes = xhr.loaded;
              this.totalBytes = xhr.total;
            } else if (xhr.loaded > 0) {
              // No Content-Length header — show indeterminate progress
              this.loadedBytes = xhr.loaded;
              this.loadProgress = Math.min(99, Math.round((xhr.loaded / (xhr.loaded + 5000000)) * 100));
            }
            this.requestUpdate();
          },
          reject,
        );
      });

      // Guard: element may have been disconnected while async loading
      if (!this.scene) {
        this.loadState = "idle";
        return;
      }

      this.modelGroup = gltf.scene;
      this.scene.add(this.modelGroup);

      // Auto-fit camera
      const box = new THREE.Box3().setFromObject(this.modelGroup!);
      const center = box.getCenter(new THREE.Vector3());
      const size = box.getSize(new THREE.Vector3());
      const maxDim = Math.max(size.x, size.y, size.z);
      const dist = maxDim / (2 * Math.tan((this.camera!.fov * Math.PI) / 360));
      this.camera!.position.set(center.x + dist, center.y + dist * 0.5, center.z + dist);
      this.controls!.target.copy(center);
      this.controls!.update();

      // Parse metadata
      this.parseMetadata();

      // Create markers
      this.createMarkers();

      this._clearSlowTimer();
      this.loadProgress = 100;
      this.loadState = "loaded";
      this.requestUpdate();
    } catch (e) {
      console.error("[Scene3D] Failed to load GLB:", e);
      this._clearSlowTimer();
      this.loadState = "error";
      this.errorMsg = "3D 场景加载失败";
      this.requestUpdate();
    }
  }

  private _clearSlowTimer() {
    if (this._slowTimer) { clearTimeout(this._slowTimer); this._slowTimer = null; }
  }

  private _formatBytes(bytes: number): string {
    if (bytes < 1024) return `${bytes} B`;
    if (bytes < 1048576) return `${(bytes / 1024).toFixed(1)} KB`;
    return `${(bytes / 1048576).toFixed(1)} MB`;
  }

  private async resolveResourceUrl(resourceId: string): Promise<string> {
    try {
      const wsId = localStorage.getItem("workspace-id") || "ws-default-001";
      const resp = await fetch(`/api/workspaces/${wsId}/resources/${resourceId}`);
      if (!resp.ok) return "";
      const json = await resp.json();
      return (json.result?.file_path) || "";
    } catch {
      return "";
    }
  }

  private parseMetadata() {

    const metadataStr = String(this.dataModel.metadata || "{}");
    try {
      const metadata = JSON.parse(metadataStr) as SceneMetadata;
      this.floors = metadata.floors || [];
      this.deviceInstances = metadata.deviceInstances || [];
    } catch {
      this.floors = [];
      this.deviceInstances = [];
    }
  }

  private createMarkers() {
    if (!this.overlayEl) return;

    // Clear old markers
    for (const m of this.markers) {
      m.element.remove();
    }
    this.markers = [];

    const deviceData = (this.dataModel.devices || []) as Array<Record<string, unknown>>;
    const deviceStatusMap = new Map<string, string>();
    for (const d of deviceData) {
      deviceStatusMap.set(String(d.deviceId || d.id), String(d.status || "offline"));
    }

    for (const inst of this.deviceInstances) {
      const el = document.createElement("div");
      el.className = "scene3d-marker";
      const status = deviceStatusMap.get(inst.deviceId) || "offline";
      const statusColors = getStatusColors();
      const color = statusColors[status] || statusColors.offline;

      const dot = document.createElement("div");
      dot.className = "scene3d-marker__dot";
      dot.style.background = color;
      dot.style.color = color;
      const label = document.createElement("div");
      label.className = "scene3d-marker__label";
      label.textContent = inst.deviceId;
      el.appendChild(dot);
      el.appendChild(label);
      el.addEventListener("click", (e) => {
        e.stopPropagation();
        if (this.onAction) {
          this.onAction("selectDevice", { deviceId: inst.deviceId });
        }
      });

      this.overlayEl.appendChild(el);
      this.markers.push({
        element: el,
        worldPos: new THREE.Vector3(...inst.position),
        floorId: inst.floorId,
        deviceId: inst.deviceId,
      });
    }
  }

  private onDataModelChanged() {
    const newModelUrl = String(this.dataModel.modelUrl || "");

    // If modelUrl changed, trigger a full reload
    if (newModelUrl && this.loadState !== "loading") {
      this.dispose();
      this.loadState = "idle";
      this.initScene();
      return;
    }

    // When model is loaded, refresh markers and floor cut on any data change
    // This handles cases where device/alarm data arrives AFTER the model loads
    if (this.modelGroup && this.loadState === "loaded") {
      this.parseMetadata();
      this.createMarkers();
      this.updateFloorCut();
    }
  }

  private updateFloorCut() {
    const floorId = String(this.dataModel.activeFloorId || "");
    this.activeFloorId = floorId || undefined;

    if (this.renderer && this.floors.length > 0) {
      const floor = this.floors.find((f) => f.id === this.activeFloorId);
      if (floor) {
        const floorHeight = 3.5;
        this.renderer.clippingPlanes = [
          new THREE.Plane(new THREE.Vector3(0, -1, 0), floor.yOffset + floorHeight),
          new THREE.Plane(new THREE.Vector3(0, 1, 0), -floor.yOffset),
        ];
      } else {
        this.renderer.clippingPlanes = [];
      }
    }

    // Filter markers
    for (const m of this.markers) {
      m.element.style.display =
        !this.activeFloorId || m.floorId === this.activeFloorId ? "flex" : "none";
    }
  }

  // ── Render loop ──

  private _animate = () => {
    this.rafId = requestAnimationFrame(this._animate);

    if (this.controls) this.controls.update();
    if (this.renderer && this.scene && this.camera) {
      this.renderer.render(this.scene, this.camera);
    }

    this.updateMarkerPositions();
  };

  private updateMarkerPositions() {
    if (!this.camera || !this.overlayEl) return;

    const rect = this.overlayEl.getBoundingClientRect();
    const width = rect.width;
    const height = rect.height;

    for (const m of this.markers) {
      if (m.element.style.display === "none") continue;

      const vec = m.worldPos.clone().project(this.camera);
      const x = (vec.x * 0.5 + 0.5) * width;
      const y = (-vec.y * 0.5 + 0.5) * height;
      const isBehind = vec.z > 1;

      m.element.style.transform = `translate(${x}px, ${y}px) translate(-50%, -100%)`;
      m.element.style.display = isBehind ? "none" : "flex";
    }
  }

  // ── Cleanup ──

  private dispose() {
    if (this.rafId) cancelAnimationFrame(this.rafId);
    this.resizeObserver?.disconnect();
    this.resizeObserver = undefined;
    for (const m of this.markers) m.element.remove();
    this.markers = [];
    this.controls?.dispose();
    if (this.modelGroup) {
      this.scene?.remove(this.modelGroup);
      this.modelGroup.traverse((obj) => {
        const mesh = obj as THREE.Mesh;
        if (mesh.geometry) mesh.geometry.dispose();
        const materials = Array.isArray(mesh.material) ? mesh.material : [mesh.material];
        for (const mat of materials) {
          if (!mat) continue;
          mat.dispose();
          for (const key of Object.keys(mat)) {
            const val = (mat as unknown as Record<string, unknown>)[key];
            if (val instanceof THREE.Texture) val.dispose();
          }
        }
      });
      this.modelGroup = undefined;
    }
    this.renderer?.dispose();
    this.renderer = undefined;
    this.scene = undefined;
    this.camera = undefined;
  }

  // ── UI handlers ──

  private handleFloorClick(floorId: string) {
    const current = String(this.dataModel.activeFloorId || "");
    const next = current === floorId ? "" : floorId;
    if (this.onAction) {
      this.onAction("setActiveFloor", { floorId: next });
    }
    // Update locally for immediate feedback
    this.dataModel = { ...this.dataModel, activeFloorId: next || undefined };
    this.updateFloorCut();
  }

  private handleRetry() {
    this.dispose();
    this.loadState = "idle";
    this.initScene();
  }

  // ── Lit render ──

  render(): TemplateResult {
    return html`
      <canvas class="scene3d-canvas"></canvas>
      <div class="scene3d-overlay"></div>

      ${this.loadState === "loading"
        ? html`<div class="scene3d-loading">
            <div class="scene3d-loading__spinner"></div>
            <div class="scene3d-loading__title">
              ${this.loadedBytes > 0
                ? "正在加载 3D 场景"
                : "正在初始化场景..."}
            </div>
            ${this.totalBytes > 0 ? html`
              <div class="scene3d-loading__progress-bar">
                <div class="scene3d-loading__progress-fill" style="width:${this.loadProgress}%"></div>
              </div>
              <div class="scene3d-loading__progress-text">${this.loadProgress}%</div>
              <div class="scene3d-loading__bytes">
                ${this._formatBytes(this.loadedBytes)} / ${this._formatBytes(this.totalBytes)}
              </div>
            ` : this.loadedBytes > 0 ? html`
              <div class="scene3d-loading__progress-bar scene3d-loading__progress-bar--indeterminate">
                <div class="scene3d-loading__progress-fill scene3d-loading__progress-fill--indeterminate"></div>
              </div>
              <div class="scene3d-loading__bytes">已下载 ${this._formatBytes(this.loadedBytes)}</div>
            ` : html`
              <div class="scene3d-loading__progress-bar scene3d-loading__progress-bar--indeterminate">
                <div class="scene3d-loading__progress-fill scene3d-loading__progress-fill--indeterminate"></div>
              </div>
              <div class="scene3d-loading__progress-text">准备加载资源...</div>
            `}
            ${this.loadSlow ? html`
              <div class="scene3d-loading__hint">
                <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" width="14" height="14">
                  <circle cx="12" cy="12" r="10"/><line x1="12" y1="8" x2="12" y2="12"/><line x1="12" y1="16" x2="12.01" y2="16"/>
                </svg>
                模型文件较大，请耐心等待...
              </div>
            ` : nothing}
          </div>`
        : ""}
      ${this.loadState === "error"
        ? html`
            <div class="scene3d-error">
              <span>${this.errorMsg}</span>
              <button @click=${this.handleRetry}>重试</button>
            </div>
          `
        : ""}

      ${this.floors.length > 0
        ? html`
            <div class="scene3d-floorbar">
              ${this.floors.map(
                (f) => html`
                  <button
                    class="scene3d-floor-btn ${this.activeFloorId === f.id
                      ? "scene3d-floor-btn--active"
                      : ""}"
                    @click=${() => this.handleFloorClick(f.id)}
                  >
                    ${f.name}
                  </button>
                `
              )}
            </div>
          `
        : ""}

    `;
  }
}

// Register custom element
customElements.define("a2ui-scene-3d", A2uiScene3D);

// ── Catalog render function ──
export function renderScene3D(
  data: Record<string, unknown>,
  onAction?: (fn: string, args: Record<string, unknown>) => void,
): TemplateResult {
  return html`<a2ui-scene-3d .dataModel=${data} .onAction=${onAction}></a2ui-scene-3d>`;
}
