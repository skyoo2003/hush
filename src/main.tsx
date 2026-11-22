import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import { App } from "./App";
import { APP_NAME } from "./brand";

document.title = APP_NAME;

const root = document.getElementById("root");
if (root) {
  createRoot(root).render(
    <StrictMode>
      <App />
    </StrictMode>,
  );
}
