// SPDX-License-Identifier: GPL-3.0-or-later
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import {
  type KeyboardEvent,
  useEffect,
  useMemo,
  useRef,
  useState,
} from "react";
import { t } from "../i18n";
import type { IndexProgress } from "../ipc/IndexProgress";
import type { IndexStatus } from "../ipc/IndexStatus";
import type { SearchHit } from "../ipc/SearchHit";
import type { SearchResults } from "../ipc/SearchResults";
import { actOnHit, runSearch, searchErrorKey } from "../searchCommands";
import {
  createSearcher,
  keyCommand,
  moveSelection,
  type SearchUpdate,
} from "../search";
import { SearchResultCard } from "./SearchResultCard";

// Must match `ingest::queue::PROGRESS_EVENT`.
const PROGRESS_EVENT = "index://progress";

export function QuickSearchModal() {
  const [query, setQuery] = useState("");
  const [update, setUpdate] = useState<SearchUpdate<SearchResults>>({
    kind: "idle",
  });
  const [selected, setSelected] = useState(0);
  const [pending, setPending] = useState(0);
  const [status, setStatus] = useState<string | null>(null);
  const [actionError, setActionError] = useState<string | null>(null);
  const input = useRef<HTMLInputElement>(null);

  const searcher = useMemo(
    () =>
      createSearcher(runSearch, (u) => {
        setUpdate(u);
        setSelected(0);
      }),
    [],
  );
  useEffect(() => searcher.set(query), [searcher, query]);
  useEffect(() => () => searcher.cancel(), [searcher]);

  // The window is created once and shown on each shortcut; select the last query so typing replaces it.
  useEffect(() => {
    const onFocus = () => input.current?.select();
    window.addEventListener("focus", onFocus);
    return () => window.removeEventListener("focus", onFocus);
  }, []);

  useEffect(() => {
    const count = (p: IndexProgress) => p.queued + p.running;
    invoke<IndexStatus>("get_index_status").then(
      (s) => setPending(count(s.progress)),
      () => {},
    );
    const unlisten = listen<IndexProgress>(PROGRESS_EVENT, (e) =>
      setPending(count(e.payload)),
    );
    return () => {
      void unlisten.then((f) => f());
    };
  }, []);

  const results = update.kind === "done" ? update.result : null;
  const hits = results?.hits ?? [];
  const current: SearchHit | undefined = hits[selected];

  const hide = () => {
    setQuery("");
    setStatus(null);
    void invoke("hide_window");
  };

  const act = async (cmd: "open" | "reveal" | "copy", hit: SearchHit) => {
    try {
      await actOnHit(cmd, hit);
      setActionError(null);
      if (cmd === "open") hide();
      if (cmd === "copy") setStatus(t("search.copied"));
    } catch (e) {
      setActionError(t(searchErrorKey(e)));
    }
  };

  const onKeyDown = (e: KeyboardEvent<HTMLInputElement>) => {
    const el = e.currentTarget;
    const cmd = keyCommand(
      {
        key: e.key,
        metaKey: e.metaKey,
        isComposing: e.nativeEvent.isComposing,
      },
      el.selectionStart !== el.selectionEnd,
    );
    if (cmd === "close") return hide();
    if (cmd === "up" || cmd === "down") {
      e.preventDefault();
      setSelected((i) => moveSelection(i, cmd, hits.length));
      return;
    }
    if (!cmd || !current) return;
    e.preventDefault();
    void act(cmd, current);
  };

  const error =
    update.kind === "error" ? t(searchErrorKey(update.error)) : actionError;

  return (
    <main className="quick-search">
      <input
        ref={input}
        type="search"
        role="combobox"
        aria-expanded={hits.length > 0}
        aria-label={t("search.label")}
        aria-controls="results"
        aria-activedescendant={current ? `hit-${selected}` : undefined}
        placeholder={t("search.placeholder")}
        // biome-ignore lint/a11y/noAutofocus: the quick-search window exists only for this input
        autoFocus
        value={query}
        onChange={(e) => {
          setQuery(e.target.value);
          setStatus(null);
          setActionError(null);
        }}
        onKeyDown={onKeyDown}
      />
      {results && results.filters.length > 0 && (
        <ul aria-label={t("search.filters")}>
          {results.filters.map((f) => (
            <li key={`${f.key}:${f.value}`}>
              {t(`filter.${f.key}`)}: {f.value}
            </li>
          ))}
        </ul>
      )}
      {error && <p role="alert">{error}</p>}
      {!query.trim() && <p>{t("search.hint")}</p>}
      {pending > 0 && query.trim() && (
        <p>{t("search.indexing", { count: pending })}</p>
      )}
      {results && hits.length === 0 && <p>{t("search.noResults")}</p>}
      {hits.length > 0 && (
        <>
          <div id="results" role="listbox" aria-label={t("search.results")}>
            {hits.map((hit, i) => (
              <SearchResultCard
                key={hit.chunkId}
                id={`hit-${i}`}
                hit={hit}
                selected={i === selected}
                onOpen={() => void act("open", hit)}
                onHover={() => setSelected(i)}
              />
            ))}
          </div>
          <p>
            <kbd>↑↓</kbd> · <kbd>Enter</kbd> {t("search.keys.open")} ·{" "}
            <kbd>⌘</kbd>
            <kbd>Enter</kbd> {t("search.keys.reveal")} · <kbd>⌘</kbd>
            <kbd>C</kbd> {t("search.keys.copy")} · <kbd>Esc</kbd>{" "}
            {t("search.keys.close")}
          </p>
        </>
      )}
      <p role="status">{status}</p>
    </main>
  );
}
