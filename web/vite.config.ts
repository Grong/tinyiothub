import path from "node:path";
import { fileURLToPath } from "node:url";
import { defineConfig } from "vite";

const here = path.dirname(fileURLToPath(import.meta.url));
const uiPort = Number(process.env.E2E_UI_PORT ?? 5173);
const fixtureOrigin = `http://localhost:${process.env.E2E_FIXTURE_PORT ?? 3002}`;

export default defineConfig(function () {
  return {
    base: "/",
    publicDir: path.resolve(here, "public"),
    build: {
      outDir: path.resolve(here, "../dist/ui"),
      emptyOutDir: true,
      sourcemap: true,
      rollupOptions: {
        output: {
          manualChunks: {
            vendor: ["lit", "@lit-labs/signals"],
            three: ["three"],
          },
        },
      },
    },
    server: {
      host: true,
      port: uiPort,
      strictPort: true,
      hmr: {
        overlay: true,
      },
      watch: {
        ignored: ['**/node_modules/**', '**/.git/**'],
      },
      proxy: {
        "/api": {
          target: fixtureOrigin,
          changeOrigin: true,
        },
        "/v1": {
          target: fixtureOrigin,
          changeOrigin: true,
        },
        "/uploads": {
          target: fixtureOrigin,
          changeOrigin: true,
        },
      },
    },
    appType: 'spa',
    optimizeDeps: {
      include: ["lit"],
    },
  };
});
