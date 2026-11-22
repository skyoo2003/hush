import { invoke } from "@tauri-apps/api/core";
import { type KeyboardEvent, useState } from "react";

export function QuickSearchModal() {
  const [query, setQuery] = useState("");

  const onKeyDown = (e: KeyboardEvent<HTMLInputElement>) => {
    if (e.key === "Escape") {
      setQuery("");
      void invoke("hide_window");
    }
  };

  return (
    <input
      type="search"
      aria-label="Search"
      // biome-ignore lint/a11y/noAutofocus: the quick-search window exists only for this input
      autoFocus
      value={query}
      onChange={(e) => setQuery(e.target.value)}
      onKeyDown={onKeyDown}
    />
  );
}
