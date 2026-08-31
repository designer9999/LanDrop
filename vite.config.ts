/// <reference types="vitest/config" />
import { defineConfig } from "vite";
import { svelte } from "@sveltejs/vite-plugin-svelte";
import tailwindcss from "@tailwindcss/vite";
import { resolve } from "path";

const host = process.env.TAURI_DEV_HOST;

export default defineConfig({
  plugins: [svelte(), tailwindcss()],
  resolve: {
    alias: {
      $lib: resolve("./src/lib"),
    },
  },
  clearScreen: false,
  server: {
    port: 1420,
    strictPort: true,
    host: host || false,
    hmr: host ? { protocol: "ws", host, port: 1421 } : undefined,
    watch: { ignored: ["**/src-tauri/**"] },
  },
  // Track the WebView2 / WebKitGTK floor rather than the bundler default.
  build: {
    target: ["es2022", "chrome105", "safari13"],
  },
  test: {
    // The suite covers the pure layer only (no DOM, no Tauri runtime).
    environment: "node",
    include: ["src/**/*.test.ts"],
  },
});
