/** Shared, non-throwing JSON preview used by Runtime event/approval surfaces. */
export function safeJson(value: unknown): string {
  if (value === undefined || value === null) return 'None';
  try {
    return JSON.stringify(value, null, 2);
  } catch {
    return 'Unavailable';
  }
}
