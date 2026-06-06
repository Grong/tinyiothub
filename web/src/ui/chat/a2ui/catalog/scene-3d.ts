import { LitElement, html, css, type TemplateResult } from "lit";
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
      align-items: center;
      justify-content: center;
      color: var(--muted, #888);
      font-size: 14px;
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
  private groundGrid?: THREE.Group;
  private rafId?: number;
  private resizeObserver?: ResizeObserver;
  private markers: Array<{ element: HTMLElement; worldPos: THREE.Vector3; floorId?: string; deviceId: string }> = [];
  private overlayEl?: HTMLElement;
  private floors: FloorInfo[] = [];
  private deviceInstances: DeviceInstance[] = [];
  private activeFloorId?: string;
  private loadState: "idle" | "loading" | "error" | "loaded" = "idle";
  private errorMsg = "";

  // ── Lit lifecycle ──

  connectedCallback() {
    super.connectedCallback();
    // Delay init until first render completes
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
    this.requestUpdate();

    const glbUrl = modelUrl;

    const loader = new GLTFLoader();
    try {
      const gltf = await new Promise<GLTF>((resolve, reject) => {
        loader.load(glbUrl, resolve, undefined, reject);
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

      this.loadState = "loaded";
      this.requestUpdate();
    } catch (e) {
      console.error("[Scene3D] Failed to load GLB:", e);
      this.loadState = "error";
      this.errorMsg = "3D 场景加载失败";
      this.requestUpdate();
    }
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
    // If modelUrl changed, reload
    const newModelUrl = String(this.dataModel.modelUrl || "");
    if (this.modelGroup && newModelUrl) {
      // For now, just update markers; full reload on resourceId change
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
        ? html`<div class="scene3d-loading">加载 3D 场景中...</div>`
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
