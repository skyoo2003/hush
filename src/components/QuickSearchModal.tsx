// SPDX-License-Identifier: GPL-3.0-or-later
import { invoke } from "@tauri-apps/api/core";
import { type KeyboardEvent, useEffect, useRef, useState } from "react";
import type { AppError } from "../ipc/AppError";
import type { SearchHit } from "../ipc/SearchHit";
import type { SearchResults } from "../ipc/SearchResults";
import { formatCitation, splitHighlights } from "../search";

const DEBOUNCE_MS = 150;

function errorText(e: unknown): string {
  switch ((e as Partial<AppError>)?.code) {
    case "model_missing":
      return "Search is off until the embedding model is installed.";
    case "source_not_found":
      return "That file was moved or deleted. It drops out after the next scan.";
    case "clipboard_failed":
      return "Could not copy the citation.";
    default:
      return "Something went wrong. Try again.";
  }
}

export function QuickSearchModal() {
  const [query, setQuery] = useState("");
  const [hits, setHits] = useState<SearchHit[] | null>(null);
  const [selected, setSelected] = useState(0);
  const [status, setStatus] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  // Only the newest request may set results; slower older ones are dropped.
  const seq = useRef(0);

  useEffect(() => {
    const mine = ++seq.current;
    if (!query.trim()) {
      setHits(null);
      setError(null);
      return;
    }
    const timer = setTimeout(() => {
      invoke<SearchResults>("search", { query }).then(
        (r) => {
          if (mine !== seq.current) return;
          setHits(r.hits);
          setSelected(0);
          setError(null);
        },
        (e) => {
          if (mine !== seq.current) return;
          setHits(null);
          setError(errorText(e));
        },
      );
    }, DEBOUNCE_MS);
    return () => clearTimeout(timer);
  }, [query]);

  const hide = () => {
    setQuery("");
    setStatus(null);
    void invoke("hide_window");
  };

  const open = async (hit: SearchHit) => {
    try {
      await invoke("open_source", { chunkId: hit.chunkId });
      hide();
    } catch (e) {
      setError(errorText(e));
    }
  };

  const copy = async (hit: SearchHit) => {
    try {
      await invoke("copy_citation", { text: formatCitation(hit) });
      setStatus("Citation copied.");
    } catch (e) {
      setError(errorText(e));
    }
  };

  const onKeyDown = (e: KeyboardEvent<HTMLInputElement>) => {
    const current = hits?.[selected];
    if (e.key === "Escape") {
      hide();
    } else if (e.key === "ArrowDown" && hits?.length) {
      e.preventDefault();
      setSelected((i) => Math.min(i + 1, hits.length - 1));
    } else if (e.key === "ArrowUp") {
      e.preventDefault();
      setSelected((i) => Math.max(i - 1, 0));
    } else if (e.key === "Enter" && current) {
      e.preventDefault();
      void (e.metaKey ? copy(current) : open(current));
    }
  };

  return (
    <main className="quick-search">
      <input
        type="search"
        role="combobox"
        aria-expanded={!!hits?.length}
        aria-label="Search"
        aria-controls="results"
        aria-activedescendant={hits?.length ? `hit-${selected}` : undefined}
        // biome-ignore lint/a11y/noAutofocus: the quick-search window exists only for this input
        autoFocus
        value={query}
        onChange={(e) => {
          setQuery(e.target.value);
          setStatus(null);
        }}
        onKeyDown={onKeyDown}
      />
      {error && <p role="alert">{error}</p>}
      {hits?.length === 0 && <p>No matching passages in your folders.</p>}
      {hits && hits.length > 0 && (
        <>
          <div id="results" role="listbox" aria-label="Results">
            {hits.map((hit, i) => (
              // Keys go through the input (aria-activedescendant); a click opens the source.
              // biome-ignore lint/a11y/useKeyWithClickEvents: keyboard handled by the search input
              <div
                key={hit.chunkId}
                id={`hit-${i}`}
                role="option"
                aria-selected={i === selected}
                tabIndex={-1}
                style={i === selected ? { outline: "2px solid Highlight" } : undefined}
                onClick={() => void open(hit)}
                onMouseEnter={() => setSelected(i)}
              >
                <strong>{hit.fileName}</strong>{" "}
                <small>
                  {hit.folderLabel}/{hit.relPath}
                  {hit.page !== null && `, p. ${hit.page}`}
                </small>
                <p>
                  {splitHighlights(hit.snippet, hit.highlights).map((s, j) =>
                    // biome-ignore lint/suspicious/noArrayIndexKey: segments are static per render
                    s.match ? <mark key={j}>{s.text}</mark> : <span key={j}>{s.text}</span>,
                  )}
                </p>
              </div>
            ))}
          </div>
          <p>
            <kbd>Enter</kbd> open · <kbd>⌘</kbd>
            <kbd>Enter</kbd> copy citation · <kbd>Esc</kbd> close
          </p>
        </>
      )}
      <p role="status">{status}</p>
    </main>
  );
}
