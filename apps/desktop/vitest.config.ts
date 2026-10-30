import { defineConfig } from 'vitest/config';
import react from '@vitejs/plugin-react';
import { fileURLToPath } from 'node:url';

export default defineConfig({
  plugins: [react()],
  server: {
    fs: {
      allow: [
        fileURLToPath(new URL('./', import.meta.url)),
        fileURLToPath(new URL('../../scenarios/examples', import.meta.url)),
        fileURLToPath(new URL('../../tests/fixtures/editor', import.meta.url)),
      ],
    },
  },
  test: { environment: 'jsdom', setupFiles: ['./src/test/setup.ts'], include: ['src/**/*.test.{ts,tsx}'] },
});
