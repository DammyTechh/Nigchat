import react from '@vitejs/plugin-react';
import { defineConfig } from 'vite';
import path from 'node:path';

/**
 * Vite rather than Next.js, deliberately.
 *
 * This client is authenticated on every route and its content is end-to-end
 * encrypted, so server-side rendering has nothing to render — there is no SEO
 * surface and no shareable public page. Choosing Next here would add a server
 * to operate, a hydration boundary to reason about, and no benefit. A static
 * SPA behind a CDN is the correct shape.
 */
export default defineConfig({
  plugins: [react()],
  resolve: {
    alias: { '@': path.resolve(__dirname, './src') },
  },
  server: {
    port: 5173,
    // Set VITE_API_URL instead if the API is not on localhost:8080.
    proxy: {
      '/v1': {
        target: process.env.VITE_API_URL ?? 'http://localhost:8080',
        changeOrigin: true,
        ws: true,
      },
    },
  },
  build: {
    target: 'es2020',
    sourcemap: true,
    rollupOptions: {
      output: {
        // Splitting the vendor chunk keeps app updates from invalidating the
        // whole bundle on every deploy.
        manualChunks: {
          react: ['react', 'react-dom'],
          icons: ['lucide-react'],
        },
      },
    },
  },
});
