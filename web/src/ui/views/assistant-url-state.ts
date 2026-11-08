/**
 * Canonical assistant hash routes.
 *
 *   #/assistant
 *   #/assistant/sessions/{encoded_session_key}
 *   #/assistant/sessions/{encoded_session_key}?view=…&device=…&at=…&event=…
 *
 * The old Runtime/Harness workbenches are gone. Their deep links redirect once
 * to the plain `#/assistant` route (see `legacyWorkbenchRedirect`); a legacy
 * Task id has no stable Task→Session mapping, so it must never become a
 * session key.
 */

import { parseHarnessHash } from './harness-url-state.js';
import { parseRuntimeHash } from './runtime-url-state.js';

export interface AssistantFocus {
  view?: string;
  device?: string;
  at?: string;
  event?: string;
}

export interface AssistantHashState {
  view: 'assistant';
  sessionKey?: string;
  focus?: AssistantFocus;
}

export const ASSISTANT_URL = '#/assistant';

function decodeSessionKey(value: string): string | null {
  try {
    const key = decodeURIComponent(value);
    return key ? key : null;
  } catch {
    return null;
  }
}

export function parseAssistantHash(hash: string): AssistantHashState | null {
  if (!hash.startsWith('#')) return null;
  const raw = hash.slice(1).replace(/^\//, '');
  const [pathPart, queryPart = ''] = raw.split('?');
  const segments = pathPart.split('/').filter(Boolean);
  if (segments[0] !== 'assistant') return null;

  const state: AssistantHashState = { view: 'assistant' };
  if (segments.length === 1) {
    // Base assistant route.
  } else if (segments.length === 3 && segments[1] === 'sessions') {
    const sessionKey = decodeSessionKey(segments[2]);
    if (!sessionKey) return null;
    state.sessionKey = sessionKey;
  } else {
    return null;
  }

  const params = new URLSearchParams(queryPart);
  const view = params.get('view');
  const device = params.get('device');
  const at = params.get('at');
  const event = params.get('event');
  if (view || device || at || event) {
    state.focus = {
      view: view ?? undefined,
      device: device ?? undefined,
      at: at ?? undefined,
      event: event ?? undefined,
    };
  }
  return state;
}

export function assistantUrl(state: AssistantHashState): string {
  const base = state.sessionKey
    ? `${ASSISTANT_URL}/sessions/${encodeURIComponent(state.sessionKey)}`
    : ASSISTANT_URL;
  if (!state.focus) return base;
  const params = new URLSearchParams();
  if (state.focus.view) params.set('view', state.focus.view);
  if (state.focus.device) params.set('device', state.focus.device);
  if (state.focus.at) params.set('at', state.focus.at);
  if (state.focus.event) params.set('event', state.focus.event);
  const query = params.toString();
  return query ? `${base}?${query}` : base;
}

/**
 * One-version compatibility: every legacy Runtime/Harness deep link lands on
 * the assistant. Invalid legacy routes return null so the pathname router can
 * continue as before.
 */
export function legacyWorkbenchRedirect(hash: string): string | null {
  return parseHarnessHash(hash) || parseRuntimeHash(hash) ? ASSISTANT_URL : null;
}
