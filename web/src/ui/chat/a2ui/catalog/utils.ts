/**
 * A2UI Catalog shared utilities.
 */

/**
 * Safely convert any value to a display string. Prevents `[object Object]` from
 * leaking into the UI when backend sends structured objects for string fields.
 *
 * Resolution order for objects:
 *   label → name → text → title → displayName → value → message → id
 *
 * Falls back to JSON.stringify for short objects, or a type hint for complex ones.
 */
export function safeStr(value: unknown, fallback = ""): string {
  if (value == null) return fallback;
  if (typeof value === "string") return value;
  if (typeof value === "number" || typeof value === "boolean") return String(value);

  if (Array.isArray(value)) {
    if (value.length === 0) return fallback;
    return value.map((v) => safeStr(v, "")).filter(Boolean).join(", ");
  }

  if (typeof value === "object") {
    const obj = value as Record<string, unknown>;
    // Try common label/name fields first
    const key = obj.label ?? obj.name ?? obj.text ?? obj.title
      ?? obj.displayName ?? obj.value ?? obj.message ?? obj.id;
    if (key != null && (typeof key === "string" || typeof key === "number")) {
      // If there's also a unit, append it
      if (obj.unit && typeof obj.unit === "string") {
        return `${String(key)} ${obj.unit}`;
      }
      return String(key);
    }
    // Fallback: short JSON
    try {
      const json = JSON.stringify(value);
      return json.length > 80 ? `[Object: ${Object.keys(obj).slice(0, 3).join(", ")}]` : json;
    } catch {
      return fallback;
    }
  }

  return fallback;
}

/**
 * Normalize a columns field that may be string[] or {label:string}[]
 */
export function normalizeColumns(raw: unknown, defaults: string[]): string[] {
  if (Array.isArray(raw)) {
    return raw.map((c) => {
      if (typeof c === "string") return c;
      if (c && typeof c === "object") {
        return safeStr((c as Record<string, unknown>).label ?? c, "");
      }
      return String(c ?? "");
    });
  }
  return defaults;
}
