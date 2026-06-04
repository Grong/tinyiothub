import { html, nothing, type TemplateResult } from "lit";
import { a2uiCatalog, type A2uiRenderer } from "./catalog/index.js";

export type A2uiSurface = {
  id: string;
  surfaceKind: "inline" | "overlay";
  components: A2uiComponent[];
};

export type A2uiComponent = {
  id: string;
  componentKind: string;
  dataModel: Record<string, unknown>;
};

export class A2uiRendererEngine {
  private surfaces: Map<string, A2uiSurface> = new Map();
  private onAction?: (functionId: string, args: Record<string, unknown>) => void;

  constructor(onAction?: (functionId: string, args: Record<string, unknown>) => void) {
    this.onAction = onAction;
  }

  handleA2uiMessage(jsonl: string): void {
    console.log("[A2UI] handleA2uiMessage called, jsonl:", jsonl.substring(0, 300));
    const lines = jsonl.split(/\\n|\n/).filter((l) => l.trim());
    console.log("[A2UI] Parsing", lines.length, "lines");
    for (const line of lines) {
      try {
        const msg = JSON.parse(line);
        console.log("[A2UI] Parsed message:", JSON.stringify(msg).substring(0, 200));
        this.handleSingleMessage(msg);
      } catch (e) {
        console.error("[A2UI] Failed to parse line:", line.substring(0, 100), e);
      }
    }
    console.log("[A2UI] Current surfaces:", Array.from(this.surfaces.keys()));
  }

  private handleSingleMessage(msg: Record<string, unknown>): void {
    if (msg.createSurface) {
      const s = msg.createSurface as Record<string, unknown>;
      const surfaceId = s.id as string;
      // Don't reset components if surface already exists (defensive against duplicate createSurface)
      const existing = this.surfaces.get(surfaceId);
      this.surfaces.set(surfaceId, {
        id: surfaceId,
        surfaceKind: ((s.surfaceKind as string) || "inline") as "inline" | "overlay",
        components: existing?.components || [],
      });
    } else if (msg.updateComponents) {
      const u = msg.updateComponents as Record<string, unknown>;
      const targetSurfaceId = u.surfaceId as string | undefined;
      const components = u.components as Array<Record<string, unknown>>;
      for (const comp of components) {
        const surfaces = targetSurfaceId
          ? [this.surfaces.get(targetSurfaceId)].filter(Boolean) as A2uiSurface[]
          : Array.from(this.surfaces.values());
        for (const surface of surfaces) {
          const idx = surface.components.findIndex((c) => c.id === comp.id);
          const { id, componentKind, dataModel, ...rest } = comp;
          const a2uiComp: A2uiComponent = {
            id: id as string,
            componentKind: componentKind as string,
            dataModel: { ...rest, ...(dataModel as Record<string, unknown> || {}) },
          };
          if (idx >= 0) {
            surface.components[idx] = a2uiComp;
          } else {
            surface.components.push(a2uiComp);
          }
        }
      }
    } else if (msg.updateDataModel) {
      const u = msg.updateDataModel as Record<string, unknown>;
      const componentId = u.componentId as string;
      const dataModel = u.dataModel as Record<string, unknown>;
      for (const surface of this.surfaces.values()) {
        const comp = surface.components.find((c) => c.id === componentId);
        if (comp) {
          comp.dataModel = { ...comp.dataModel, ...dataModel };
        }
      }
    } else if (msg.deleteSurface) {
      const d = msg.deleteSurface as Record<string, unknown>;
      this.surfaces.delete(d.id as string);
    }
  }

  renderSurface(surfaceId: string): TemplateResult | typeof nothing {
    const surface = this.surfaces.get(surfaceId);
    console.log("[A2UI] renderSurface called for:", surfaceId, "found:", !!surface, "all surfaces:", Array.from(this.surfaces.keys()));
    if (!surface) return nothing;

    return html`
      <div class="a2ui-surface a2ui-surface--${surface.surfaceKind}">
        ${surface.components.map((comp) => this.renderComponent(comp))}
      </div>
    `;
  }

  renderAllSurfaces(): TemplateResult[] {
    const results: TemplateResult[] = [];
    for (const [id] of this.surfaces) {
      const rendered = this.renderSurface(id);
      if (rendered !== nothing) {
        results.push(rendered as TemplateResult);
      }
    }
    return results;
  }

  private renderComponent(comp: A2uiComponent): TemplateResult {
    const renderer: A2uiRenderer | undefined = a2uiCatalog[comp.componentKind];
    if (!renderer) {
      return html`<div class="a2ui-unknown">Unknown component: ${comp.componentKind}</div>`;
    }
    console.log("[A2UI] renderComponent:", comp.componentKind, "dataModel keys:", Object.keys(comp.dataModel));
    return renderer(comp.dataModel, this.onAction);
  }

  clear(): void {
    this.surfaces.clear();
  }

  hasSurfaces(): boolean {
    return this.surfaces.size > 0;
  }

  getSurfaceIds(): string[] {
    return Array.from(this.surfaces.keys());
  }
}
