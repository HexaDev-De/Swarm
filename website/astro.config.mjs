import { defineConfig } from 'astro/config';

export default defineConfig({
  output: 'static',
  devToolbar: { enabled: false },
  outDir: './dist',
  server: { host: '127.0.0.1', port: 4173 },
  vite: {
    server: { strictPort: true },
    preview: { strictPort: true },
    build: { target: 'es2022', assetsInlineLimit: 0 },
  },
});
