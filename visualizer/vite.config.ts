import { defineConfig } from 'vite';
import react from '@vitejs/plugin-react';

// base relative : le visualiseur construit fonctionne aussi bien servi par
// `chem viz` qu'ouvert depuis un autre repertoire.
export default defineConfig({
  plugins: [react()],
  base: './',
  build: {
    outDir: 'dist',
    sourcemap: true,
  },
});
