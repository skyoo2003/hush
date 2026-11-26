// SPDX-License-Identifier: GPL-3.0-or-later
/// <reference types="vitest/config" />
import { defineConfig, loadEnv, type Plugin } from "vite";

// UI harness (src/dev): with VITE_MOCK_IPC=1 the dev server runs src/dev/boot.tsx before main.tsx,
// which mocks the IPC and serves /__harness. `apply: "serve"` keeps it out of every build;
// CI checks dist/ for the mock's marker.
const mockIpc: Plugin = {
  name: "mock-ipc",
  apply: "serve",
  transformIndexHtml: (html) =>
    html.replace(
      '<script type="module" src="/src/main.tsx">',
      '<script type="module" src="/src/dev/boot.tsx"></script>\n    $&',
    ),
};

// No @vitejs/plugin-react: its inline refresh preamble violates CSP script-src 'self'.
// Vite 8 transforms with Oxc (the `esbuild` option is deprecated), so JSX is configured here.
export default defineConfig(({ mode }) => ({
  clearScreen: false,
  oxc: { jsx: { runtime: "automatic" } },
  plugins: loadEnv(mode, ".", "VITE_").VITE_MOCK_IPC === "1" ? [mockIpc] : [],
  server: {
    port: 1420,
    strictPort: true,
    watch: { ignored: ["**/src-tauri/**"] },
  },
  build: { target: "safari15" },
  test: { setupFiles: ["src/ui/test-setup.ts"] },
}));
