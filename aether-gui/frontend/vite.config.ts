import { defineConfig } from "vite";
import { svelte } from "@sveltejs/vite-plugin-svelte";

// Tauri dev server must use a fixed port (see tauri.conf.json devUrl)
// and strictPort so a clash fails loudly instead of silently moving.
export default defineConfig({
  plugins: [svelte()],
  clearScreen: false,
  server: {
    port: 1420,
    strictPort: true
  },
  build: {
    target: "es2021"
  }
});
