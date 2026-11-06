import path from 'path';
import { defineConfig } from 'vitest/config';

export default defineConfig({
  test: {
    globals: true,
    environment: 'node',
    include: ['tests/**/*.test.ts'],
  },
  resolve: {
    alias: {
      '@': path.resolve(import.meta.dirname, './src'),
      '@core': path.resolve(import.meta.dirname, './src/core'),
      '@features': path.resolve(import.meta.dirname, './src/features'),
      '@generated': path.resolve(import.meta.dirname, './generated'),
      '@tests': path.resolve(import.meta.dirname, './tests'),
    },
  },
});
