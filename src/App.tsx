import { getCurrentWindow } from "@tauri-apps/api/window";
import { APP_NAME } from "./brand";
import { QuickSearchModal } from "./components/QuickSearchModal";

// Both windows load index.html; the window label picks the UI.
const label = getCurrentWindow().label;

export function App() {
  if (label === "quick-search") {
    return <QuickSearchModal />;
  }
  return (
    <main>
      <h1>{APP_NAME}</h1>
    </main>
  );
}
