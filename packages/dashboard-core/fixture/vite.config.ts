import { defineConfig } from 'vite';
import { viteSingleFile } from 'vite-plugin-singlefile';
import solidPlugin from 'vite-plugin-solid';

export default defineConfig({
  root: 'fixture',
  plugins: [solidPlugin(), viteSingleFile()],
  build: { outDir: 'dist', emptyOutDir: true, cssCodeSplit: false, assetsInlineLimit: 100000000 },
});
