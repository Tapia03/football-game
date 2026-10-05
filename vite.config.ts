import { defineConfig } from 'vite';
import { svelte } from '@sveltejs/vite-plugin-svelte';

// SharedArrayBuffer (engine worker <-> renderer, Phase 6) requires cross-origin
// isolation. Dev and preview servers mirror frontend/public/_headers so local
// runs behave exactly like Cloudflare Pages.
const crossOriginIsolation = {
  'Cross-Origin-Opener-Policy': 'same-origin',
  'Cross-Origin-Embedder-Policy': 'require-corp',
} as const;

export default defineConfig({
  root: 'frontend',
  plugins: [svelte()],
  // The engine worker is an ES module (it imports the wasm-bindgen glue).
  worker: { format: 'es' },
  build: {
    outDir: '../dist',
    emptyOutDir: true,
    target: 'es2022',
  },
  server: {
    port: 5173,
    strictPort: true,
    headers: crossOriginIsolation,
  },
  preview: {
    port: 4173,
    strictPort: true,
    headers: crossOriginIsolation,
  },
});
