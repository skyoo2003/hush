// SPDX-License-Identifier: GPL-3.0-or-later
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { useCallback, useEffect, useState } from "react";
import type { AddedFolder } from "../ipc/AddedFolder";
import type { AppError } from "../ipc/AppError";
import type { FolderInfo } from "../ipc/FolderInfo";
import type { IndexProgress } from "../ipc/IndexProgress";
import type { IndexStatus } from "../ipc/IndexStatus";

// Must match `ingest::queue::PROGRESS_EVENT`.
const PROGRESS_EVENT = "index://progress";

function errorText(e: unknown): string {
  switch ((e as Partial<AppError>)?.code) {
    case "folder_overlap":
      return "That folder is already watched, or is inside or contains a watched folder.";
    case "folder_not_found":
      return "That folder no longer exists.";
    case "folder_unreadable":
      return "Locite cannot read that folder. Check its permissions.";
    default:
      return "Something went wrong. Try again.";
  }
}

function progressText(indexing: boolean, p: IndexProgress | null): string {
  if (!indexing || !p) return "Indexing starts once the embedding model loads.";
  if (p.paused) return "Indexing paused.";
  const pending = p.queued + p.running;
  const failed = p.failed ? `, ${p.failed} failed` : "";
  return pending
    ? `Indexing: ${pending} left, ${p.done} done${failed}.`
    : `Up to date${failed}.`;
}

export function FolderList() {
  const [folders, setFolders] = useState<FolderInfo[]>([]);
  const [status, setStatus] = useState<IndexStatus | null>(null);
  const [busy, setBusy] = useState(false);
  const [notice, setNotice] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);

  const refresh = useCallback(() => {
    invoke<FolderInfo[]>("list_folders").then(setFolders, () =>
      setError("Could not read the folder list."),
    );
  }, []);

  useEffect(() => {
    refresh();
    invoke<IndexStatus>("get_index_status").then(setStatus, () => {});
    // Counts change as documents finish, so the list refreshes with each progress event.
    const unlisten = listen<IndexProgress>(PROGRESS_EVENT, (e) => {
      setStatus({ indexing: true, progress: e.payload });
      refresh();
    });
    return () => {
      void unlisten.then((f) => f());
    };
  }, [refresh]);

  const run = async (action: () => Promise<void>) => {
    setBusy(true);
    setError(null);
    setNotice(null);
    try {
      await action();
    } catch (e) {
      setError(errorText(e));
    } finally {
      setBusy(false);
      refresh();
    }
  };

  const add = () =>
    run(async () => {
      const added = await invoke<AddedFolder | null>("add_watched_folder");
      if (added?.warning === "broad_path") {
        setNotice("This is a large folder. Indexing it may take a while.");
      }
    });

  const remove = (f: FolderInfo) =>
    run(async () => {
      await invoke("remove_watched_folder", { id: f.id });
    });

  return (
    <section aria-labelledby="folders-heading">
      <h2 id="folders-heading">Folders</h2>
      <p role="status">{progressText(status?.indexing ?? false, status?.progress ?? null)}</p>
      {folders.length === 0 ? (
        <p>No folders yet. Add one to start indexing.</p>
      ) : (
        <ul>
          {folders.map((f) => (
            <li key={f.id}>
              <strong>{f.label}</strong> <small title={f.path}>{f.path}</small>{" "}
              <span>
                {f.documents} documents
                {f.failed > 0 && `, ${f.failed} failed`}
                {f.status === "unavailable" && " (unavailable)"}
              </span>{" "}
              <button
                type="button"
                disabled={busy}
                onClick={() => void remove(f)}
                aria-label={`Remove ${f.label}`}
              >
                Remove
              </button>
            </li>
          ))}
        </ul>
      )}
      <button type="button" disabled={busy} onClick={() => void add()}>
        Add folder…
      </button>
      {notice && <p>{notice}</p>}
      {error && <p role="alert">{error}</p>}
    </section>
  );
}
