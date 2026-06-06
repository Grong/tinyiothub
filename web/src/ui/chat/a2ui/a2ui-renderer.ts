import { html, nothing, type TemplateResult } from "lit";
import { a2uiCatalog, type A2uiRenderer } from "./catalog/index.js";

export type A2uiSurface = {
  id: string;
  surfaceKind: "inline" | "overlay" | "stage" | "insight";
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
    const messages = this._parseJsonl(jsonl);
    console.log("[A2UI] Parsing", messages.length, "messages");
    for (const msg of messages) {
      console.log("[A2UI] Parsed message:", JSON.stringify(msg).substring(0, 200));
      this.handleSingleMessage(msg);
    }
    console.log("[A2UI] Current surfaces:", Array.from(this.surfaces.keys()));
  }

  /** Parse JSONL by tracking JSON nesting depth — handles any separator. */
  private _parseJsonl(jsonl: string): Array<Record<string, unknown>> {
    const results: Array<Record<string, unknown>> = [];
    let i = 0;
    while (i < jsonl.length) {
      // Skip whitespace and separator chars
      while (i < jsonl.length && /[\s⏎]/.test(jsonl[i])) i++;
      if (i >= jsonl.length) break;

      // Skip garbage characters that are not JSON object/array starts
      if (jsonl[i] !== "{" && jsonl[i] !== "[") {
        i++;
        continue;
      }

      let depth = 0;
      let inString = false;
      let escape = false;
      let j = i;

      for (; j < jsonl.length; j++) {
        const c = jsonl[j];
        if (escape) {
          escape = false;
          continue;
        }
        if (c === "\\") {
          escape = true;
          continue;
        }
        if (c === '"' && !inString) {
          inString = true;
          continue;
        }
        if (c === '"' && inString) {
          inString = false;
          continue;
        }
        if (!inString) {
          if (c === "{" || c === "[") depth++;
          if (c === "}" || c === "]") depth--;
          if (depth === 0 && (c === "}" || c === "]")) {
            j++; // Include the closing brace
            break;
          }
        }
      }

      const slice = jsonl.slice(i, j);
      try {
        const obj = JSON.parse(slice);
        results.push(obj as Record<string, unknown>);
      } catch (_e) {
        // JSON was malformed (e.g. LLM miscounted closing braces).
        // Skip to the next JSONL line and continue.
        console.error("[A2UI] Failed to parse JSON:", slice.substring(0, 100), String(_e));
        const nl = jsonl.indexOf("\n", i + 1);
        if (nl !== -1) {
          i = nl;
          continue;
        }
      }

      i = j;
    }
    return results;
  }

  private handleSingleMessage(msg: Record<string, unknown>): void {
    if (msg.createSurface) {
      const s = msg.createSurface as Record<string, unknown>;
      const surfaceId = s.id as string;
      // Don't reset components if surface already exists (defensive against duplicate createSurface)
      const existing = this.surfaces.get(surfaceId);
      this.surfaces.set(surfaceId, {
        id: surfaceId,
        surfaceKind: ((s.surfaceKind as string) || "inline") as A2uiSurface["surfaceKind"],
        components: existing?.components || [],
      });
    }
    if (msg.updateComponents) {
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
    }
    if (msg.updateDataModel) {
      const u = msg.updateDataModel as Record<string, unknown>;
      const componentId = u.componentId as string;
      const dataModel = u.dataModel as Record<string, unknown>;
      for (const surface of this.surfaces.values()) {
        const comp = surface.components.find((c) => c.id === componentId);
        if (comp) {
          comp.dataModel = { ...comp.dataModel, ...dataModel };
        }
      }
    }
    if (msg.deleteSurface) {
      const d = msg.deleteSurface as Record<string, unknown>;
      this.surfaces.delete(d.id as string);
    }
  }

  renderSurface(surfaceId: string): TemplateResult | typeof nothing {
    const surface = this.surfaces.get(surfaceId);
    console.log("[A2UI] renderSurface called for:", surfaceId, "found:", !!surface, "all surfaces:", Array.from(this.surfaces.keys()));
    if (!surface) return nothing;

    // Auto-group consecutive StatCards into a row
    const parts: TemplateResult[] = [];
    let statBatch: A2uiComponent[] = [];
    for (const comp of surface.components) {
      if (comp.componentKind === "StatCard") {
        statBatch.push(comp);
      } else {
        if (statBatch.length > 1) {
          parts.push(html`
            <div class="a2ui-stat-row a2ui-stat-row--auto">
              ${statBatch.map((c) => this.renderComponent(c))}
            </div>
          `);
        } else if (statBatch.length === 1) {
          parts.push(this.renderComponent(statBatch[0]));
        }
        statBatch = [];
        parts.push(this.renderComponent(comp));
      }
    }
    if (statBatch.length > 1) {
      parts.push(html`
        <div class="a2ui-stat-row a2ui-stat-row--auto">
          ${statBatch.map((c) => this.renderComponent(c))}
        </div>
      `);
    } else if (statBatch.length === 1) {
      parts.push(this.renderComponent(statBatch[0]));
    }

    return html`
      <div class="a2ui-surface a2ui-surface--${surface.surfaceKind}">
        ${parts}
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

  getSurfaceKind(surfaceId: string): A2uiSurface["surfaceKind"] | undefined {
    return this.surfaces.get(surfaceId)?.surfaceKind;
  }

  getStageSurfaceIds(): string[] {
    return Array.from(this.surfaces.values())
      .filter((s) => s.surfaceKind === "stage")
      .map((s) => s.id);
  }

  getInsightSurfaceIds(): string[] {
    return Array.from(this.surfaces.values())
      .filter((s) => s.surfaceKind === "insight")
      .map((s) => s.id);
  }

  getInlineSurfaceIds(): string[] {
    return Array.from(this.surfaces.values())
      .filter((s) => s.surfaceKind === "inline")
      .map((s) => s.id);
  }

  getCanvasSurfaceIds(): string[] {
    return Array.from(this.surfaces.values())
      .filter((s) => s.surfaceKind === "stage" || s.surfaceKind === "insight" || s.surfaceKind === "inline")
      .map((s) => s.id);
  }

  getSurfaceComponentKinds(surfaceId: string): string[] {
    const surface = this.surfaces.get(surfaceId);
    return surface?.components.map((c) => c.componentKind) || [];
  }

  renderSurfacesByKind(kind: A2uiSurface["surfaceKind"]): TemplateResult[] {
    const results: TemplateResult[] = [];
    for (const surface of this.surfaces.values()) {
      if (surface.surfaceKind !== kind) continue;
      const rendered = this.renderSurface(surface.id);
      if (rendered !== nothing) {
        results.push(rendered as TemplateResult);
      }
    }
    return results;
  }
}
