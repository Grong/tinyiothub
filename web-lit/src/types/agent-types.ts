/**
 * Agent chat + A2UI type definitions
 */

/** A2UI message types (v0.10 protocol) */
export type A2uiMessage =
  | { type: 'createSurface'; payload: { surfaceId: string; title?: string; layout?: string } }
  | { type: 'updateComponents'; payload: { surfaceId: string; components: A2uiComponentDescriptor[] } }
  | { type: 'updateDataModel'; payload: Record<string, unknown> }
  | { type: 'deleteSurface'; payload: { surfaceId: string } }
  | { type: 'callFunction'; payload: { functionId: string; args: Record<string, unknown> } }
  | { type: 'actionResponse'; payload: { actionId: string; result: unknown } }

/** A2UI component descriptor */
export interface A2uiComponentDescriptor {
  id: string
  type: string
  props?: Record<string, unknown>
  children?: A2uiComponentDescriptor[]
}

/** A2UI surface state */
export interface A2uiSurfaceState {
  surfaceId: string
  title?: string
  components: A2uiComponentDescriptor[]
  dataModel: Record<string, unknown>
}

/** Chat message */
export interface ChatMessage {
  id: string
  role: 'user' | 'assistant'
  content: string
  timestamp: number
  surfaces?: Map<string, A2uiSurfaceState>
  isStreaming?: boolean
}

/** SSE event types from the agent endpoint */
export type SseEvent =
  | { type: 'delta'; content: string }
  | { type: 'a2ui'; message: A2uiMessage }
  | { type: 'final'; content: string }

/** Device property for IoT components */
export interface DeviceProperty {
  name: string
  displayName?: string
  value: string
  unit?: string
  currentValue?: string
  dataType?: string
}
