import path from "node:path";
import { fileURLToPath } from "node:url";
import { defineConfig } from "vite";

const here = path.dirname(fileURLToPath(import.meta.url));

export default defineConfig(function () {
  return {
    base: "/",
    publicDir: path.resolve(here, "public"),
    build: {
      outDir: path.resolve(here, "../dist/ui"),
      emptyOutDir: true,
      sourcemap: true,
    },
    server: {
      host: true,
      port: 5173,
      strictPort: true,
      hmr: {
        overlay: true,
      },
      watch: {
        ignored: ['**/node_modules/**', '**/.git/**'],
      },
      proxy: {
        "/api": {
          target: "http://localhost:3002",
          changeOrigin: true,
        },
        "/v1": {
          target: "http://localhost:3002",
          changeOrigin: true,
        }
      },
    },
    appType: 'spa',
    optimizeDeps: {
      include: ["lit"],
    },
  };
});
