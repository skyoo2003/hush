// SPDX-License-Identifier: GPL-3.0-or-later
// Search state for the library window: the toolbar field drives it, the Search page shows it.
import { type KeyboardEvent, useEffect, useMemo, useState } from "react";
import { t } from "../i18n";
import type { SearchHit } from "../ipc/SearchHit";
import type { SearchResults } from "../ipc/SearchResults";
import {
	createSearcher,
	keyCommand,
	moveSelection,
	type SearchUpdate,
} from "../search";
import { actOnHit, runSearch, searchErrorKey } from "../searchCommands";

export type LibrarySearch = ReturnType<typeof useLibrarySearch>;

export function useLibrarySearch(query: string) {
	const [update, setUpdate] = useState<SearchUpdate<SearchResults>>({
		kind: "idle",
	});
	const [selected, setSelected] = useState(0);
	const [status, setStatus] = useState<string | null>(null);
	const [actionError, setActionError] = useState<string | null>(null);

	const searcher = useMemo(
		() =>
			createSearcher(runSearch, (u) => {
				setUpdate(u);
				setSelected(0);
			}),
		[],
	);
	useEffect(() => {
		searcher.set(query);
		setStatus(null);
		setActionError(null);
	}, [searcher, query]);
	useEffect(() => () => searcher.cancel(), [searcher]);

	const results = update.kind === "done" ? update.result : null;
	const hits = results?.hits ?? [];
	const current: SearchHit | undefined = hits[selected];

	const act = async (cmd: "open" | "reveal" | "copy", hit: SearchHit) => {
		try {
			await actOnHit(cmd, hit);
			setActionError(null);
			setStatus(cmd === "copy" ? t("search.copied") : null);
		} catch (e) {
			setActionError(t(searchErrorKey(e)));
		}
	};

	/** The toolbar field's keys: ↑↓ select, Return opens, ⌘Return reveals, ⌘C cites. */
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
		if (cmd === "close" || !cmd) return;
		if (cmd === "up" || cmd === "down") {
			e.preventDefault();
			setSelected((i) => moveSelection(i, cmd, hits.length));
			return;
		}
		if (!current) return;
		e.preventDefault();
		void act(cmd, current);
	};

	const error =
		update.kind === "error" ? t(searchErrorKey(update.error)) : actionError;

	return {
		update,
		results,
		hits,
		selected,
		setSelected,
		current,
		act,
		onKeyDown,
		error,
		status,
	};
}
