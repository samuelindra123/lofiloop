import { defineConfig } from "vite";
import { svelte } from "@sveltejs/vite-plugin-svelte";

// Tauri menyajikan hasil build sebagai berkas statis, jadi base harus relatif.
export default defineConfig({
  plugins: [svelte()],
  base: "./",
  build: {
    outDir: "dist",
    emptyOutDir: true,
    target: "chrome110",
    sourcemap: false,
  },
  server: {
    port: 5183,
    strictPort: true,
  },
});
