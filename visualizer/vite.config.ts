import { defineConfig } from 'vite';
import { fileURLToPath } from 'node:url';
import { resolve } from 'node:path';

const here = fileURLToPath(new URL('.', import.meta.url));

// Renderer-only build. The Electron main/preload bundles are produced by esbuild
// (see package.json), because they need the CommonJS/node target Electron expects.
export default defineConfig({
  root: resolve(here, 'src/renderer'),
  base: './',
  server: { port: 5273, strictPort: true },
  build: {
    outDir: resolve(here, 'dist/renderer'),
    emptyOutDir: true,
    target: 'chrome120',
    sourcemap: true,
  },
});
