export function recordOf(value: unknown): Record<string, unknown> | null {
  return typeof value === 'object' && value !== null && !Array.isArray(value)
    ? (value as Record<string, unknown>)
    : null;
}

export function parseJson(value: unknown): unknown {
  if (typeof value !== 'string') return value;
  const text = value.trim();
  if (!text) return null;
  try {
    return JSON.parse(text);
  } catch {
    return null;
  }
}

export function asString(value: unknown): string {
  return typeof value === 'string' ? value : '';
}

export function displayValue(value: unknown): string {
  if (value === null || value === undefined) return '—';
  if (typeof value === 'string') return value;
  if (typeof value === 'number' || typeof value === 'boolean') return String(value);
  return JSON.stringify(value);
}

export function clampText(value: string, max = 80): string {
  const text = value.replace(/\s+/g, ' ').trim();
  return text.length > max ? `${text.slice(0, max)}…` : text;
}

export interface DeviceSource {
  label: string;
  finding: string;
  at: string;
}

export interface DeviceFacts {
  id: string;
  name: string;
  zone: string;
  online?: boolean;
  health: string;
  metrics: Array<{ label: string; value: string }>;
  sources: DeviceSource[];
  actions: Array<{ name: string; description: string }>;
}

const DEVICE_ID_KEYS = ['thingId', 'thing_id', 'deviceId', 'device_id', 'id'];
const DEVICE_NAME_KEYS = ['displayName', 'display_name', 'name', 'thingName', 'thing_name'];

function firstString(record: Record<string, unknown> | null, keys: string[]): string {
  if (!record) return '';
  for (const key of keys) {
    const value = record[key];
    if (typeof value === 'string' && value.trim()) return value.trim();
  }
  return '';
}

export function deviceFacts(result: unknown, args: unknown): DeviceFacts | null {
  const record = recordOf(parseJson(result)) ?? recordOf(result);
  const argsRecord = recordOf(args);
  const id = firstString(record, DEVICE_ID_KEYS) || firstString(argsRecord, DEVICE_ID_KEYS);
  const name =
    firstString(record, DEVICE_NAME_KEYS) ||
    firstString(argsRecord, DEVICE_NAME_KEYS) ||
    id;
  if (!id && !name) return null;
  const state = recordOf(record?.state);
  const metrics = state
    ? Object.entries(state).map(([label, value]) => ({ label, value: clampText(displayValue(value), 32) }))
    : [];
  const online = typeof record?.online === 'boolean' ? record.online : undefined;
  const sources = (Array.isArray(record?.sources) ? record.sources : [])
    .map(recordOf)
    .filter((source): source is Record<string, unknown> => source !== null)
    .map((source) => {
      const evidence = recordOf(source.evidence);
      const detections = Array.isArray(source.data) ? source.data.length : 0;
      return {
        label: firstString(source, ['source', 'sourceId', 'source_id']) || firstString(evidence, ['source']) || '现场画面',
        finding: firstString(source, ['text', 'summary', 'analysis', 'description']) || (detections > 0 ? `识别 ${detections} 个目标` : ''),
        at: firstString(source, ['mediaTime', 'media_time']) || firstString(evidence, ['mediaTime', 'media_time']),
      };
    });
  const actions = (Array.isArray(record?.actions) ? record.actions : [])
    .map(recordOf)
    .filter((action): action is Record<string, unknown> => action !== null)
    .map((action) => ({
      name: firstString(action, ['name', 'actionName', 'action_name', 'id']),
      description: firstString(action, ['description', 'label', 'summary']),
    }))
    .filter((action) => action.name);
  return {
    id,
    name: name || id,
    zone: firstString(record, ['zone', 'position', 'address']),
    online,
    health: firstString(record, ['health']) || 'unknown',
    metrics,
    sources,
    actions,
  };
}

export interface ThingListFacts {
  total: number;
  items: Array<{ id: string; name: string; online?: boolean }>;
}

export function thingListFacts(result: unknown): ThingListFacts | null {
  const record = recordOf(parseJson(result)) ?? recordOf(result);
  if (!record) return null;
  const items = (Array.isArray(record.items)
    ? record.items
    : Array.isArray(record.things)
      ? record.things
      : Array.isArray(record.devices)
        ? record.devices
        : []) as unknown[];
  const records = items.map(recordOf).filter((item): item is Record<string, unknown> => item !== null);
  if (records.length === 0 && !Array.isArray(record.items)) return null;
  return {
    total: typeof record.total === 'number' ? record.total : records.length,
    items: records.map((item) => ({
      id: firstString(item, DEVICE_ID_KEYS),
      name: firstString(item, DEVICE_NAME_KEYS),
      online: typeof item.online === 'boolean' ? item.online : undefined,
    })),
  };
}

export function propertyFacts(result: unknown): { property: string; value: string } | null {
  const record = recordOf(parseJson(result)) ?? recordOf(result);
  if (!record) return null;
  const property = firstString(record, ['propertyName', 'property_name', 'property', 'name']);
  if (!property) return null;
  const value = displayValue(record.currentValue);
  const unit = typeof record.unit === 'string' ? ` ${record.unit}` : '';
  return { property, value: `${value}${unit}` };
}
