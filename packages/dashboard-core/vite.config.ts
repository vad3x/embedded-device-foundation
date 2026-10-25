import { defineConfig } from 'vite';
import solidPlugin from 'vite-plugin-solid';

export default defineConfig({
  plugins: [solidPlugin()],
  build: {
    lib: { entry: 'src/index.ts', formats: ['es'], fileName: 'index' },
    rollupOptions: { external: (id) => id === 'solid-js' || id.startsWith('solid-js/') },
    outDir: 'dist',
    emptyOutDir: true,
  },
});
