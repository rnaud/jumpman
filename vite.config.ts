import { defineConfig } from 'vite';

export default defineConfig({
  server: {
    host: '127.0.0.1',
    port: 5173,
  },
  build: {
    target: 'es2022',
    // Keep small GLBs out of the JS bundle; models load only when requested.
    assetsInlineLimit: 0,
    rollupOptions: {
      output: { manualChunks: id => id.includes('/node_modules/') && id.includes('/three/') ? 'three' : undefined },
    },
    chunkSizeWarningLimit: 900,
  },
});
