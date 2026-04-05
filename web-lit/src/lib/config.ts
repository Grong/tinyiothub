/// <reference types="vite/client" />

// Vite environment variables
// Use import.meta.env.VITE_* for client-side env vars

export const API_PREFIX = (import.meta as any).env?.VITE_API_PREFIX || '/api/v1'
export const PUBLIC_API_PREFIX = (import.meta as any).env?.VITE_PUBLIC_API_PREFIX || '/api/v1'
export const IS_CE_EDITION = (import.meta as any).env?.VITE_EDITION === 'SELF_HOSTED'
