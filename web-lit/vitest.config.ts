import { defineConfig } from 'vitest/config'
import path from 'node:path'
import { fileURLToPath } from 'node:url'

const here = path.dirname(fileURLToPath(import.meta.url))

export default defineConfig({
  test: {
    environment: 'jsdom',
    globals: true,
    setupFiles: [],
    include: ['src/**/*.test.ts'],
    exclude: ['src/**/*.spec.ts'],
  },
  resolve: {
    alias: {
      '@': path.resolve(here, './src'),
    },
  },
})
