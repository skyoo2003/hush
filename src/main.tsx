// SPDX-License-Identifier: GPL-3.0-or-later
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

const root = document.getElementById("root");
if (root) {
  createRoot(root).render(
    <StrictMode>
      <App />
    </StrictMode>,
  );
}
