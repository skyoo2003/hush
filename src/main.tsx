// SPDX-License-Identifier: GPL-3.0-or-later
import { getCurrentWindow } from "@tauri-apps/api/window";
import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import { App } from "./App";
import { APP_NAME } from "./brand";
import "./ui/tokens.css";
import "./ui/base.css";
import "./app.css";
import { startAppearance } from "./ui/appearance";
import { trackWindowFocus } from "./ui/window";

document.title = APP_NAME;
// On failure the window keeps the CSS defaults: system theme, the app's own accent.
startAppearance().catch(() => {});
trackWindowFocus();
// The main window has native sidebar vibrancy (tauri.conf.json); the harness has none.
if (getCurrentWindow().label === "main" && !import.meta.env.VITE_MOCK_IPC) {
  document.documentElement.dataset.vibrancy = "";
}

const root = document.getElementById("root");
if (root) {
  createRoot(root).render(
    <StrictMode>
      <App />
    </StrictMode>,
  );
}
