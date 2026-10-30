import wails from '@wailsio/runtime/plugins/vite';
import { defineConfig } from 'vite';
import react from '@vitejs/plugin-react';
import tailwindcss from '@tailwindcss/vite';
import process from 'node:process';
import { fileURLToPath } from 'node:url';

// https://vite.dev/config/
export default defineConfig(({ mode }) => ({
  plugins: [react(), tailwindcss(), ...(mode !== 'browser' ? [wails('./src/lib/desktop/generated')] : [])],
  define: {
    'import.meta.env.VITE_DESKTOP_HOST': JSON.stringify(mode === 'browser' ? 'browser' : 'wails'),
  },
  build: mode !== 'browser' ? { outDir: '../../go/frontend/dist', emptyOutDir: true } : undefined,
  resolve: { alias: { '@': fileURLToPath(new URL('./src', import.meta.url)) } },

  clearScreen: false,
  server: {
    fs: {
      allow: [
        fileURLToPath(new URL('./', import.meta.url)),
        fileURLToPath(new URL('../../scenarios/examples', import.meta.url)),
      ],
    },
    port: mode !== 'browser' ? Number(process.env.WAILS_VITE_PORT) || 9245 : 5173,
    strictPort: true,
    host: '127.0.0.1',
  },
}));
