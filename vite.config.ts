import { defineConfig } from "vite";

// No @vitejs/plugin-react: its inline refresh preamble violates CSP script-src 'self'.
// Vite 8 transforms with Oxc (the `esbuild` option is deprecated), so JSX is configured here.
export default defineConfig({
  clearScreen: false,
  oxc: { jsx: { runtime: "automatic" } },
  server: {
    port: 1420,
    strictPort: true,
    watch: { ignored: ["**/src-tauri/**"] },
  },
  build: { target: "safari15" },
});
