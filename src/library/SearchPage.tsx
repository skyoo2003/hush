// SPDX-License-Identifier: GPL-3.0-or-later
import { SearchX, TextSearch } from "lucide-react";
import { useEffect, useRef } from "react";
import { SearchResultCard } from "../components/SearchResultCard";
import { t } from "../i18n";
import { Banner } from "../ui/Banner";
import { EmptyState } from "../ui/EmptyState";
import { Skeleton } from "../ui/Skeleton";
import { Tag } from "../ui/Tag";
import type { LibrarySearch } from "./useLibrarySearch";

type Props = { query: string; search: LibrarySearch; pending: number };

/** Results for the toolbar field. Keys go through the field (aria-activedescendant). */
export function SearchPage({ query, search, pending }: Props) {
	const { update, results, hits, selected, setSelected, act, error, status } =
		search;
	const list = useRef<HTMLDivElement>(null);

	useEffect(() => {
		list.current
			?.querySelector(`#hit-${selected}`)
			?.scrollIntoView({ block: "nearest" });
	}, [selected]);

	if (!query.trim())
		return (
			<EmptyState
				icon={TextSearch}
				title={t("search.placeholder")}
				description={t("search.hint")}
			/>
		);
	return (
		<div className="library-search">
			{error && <Banner tone="error">{error}</Banner>}
			{pending > 0 && (
				<Banner tone="info">{t("search.indexing", { count: pending })}</Banner>
			)}
			{results && results.filters.length > 0 && (
				<ul className="library-filters" aria-label={t("search.filters")}>
					{results.filters.map((f) => (
						<li key={`${f.key}:${f.value}`}>
							<Tag label={`${t(`filter.${f.key}`)}: ${f.value}`} />
						</li>
					))}
				</ul>
			)}
			{update.kind === "idle" && <Skeleton lines={6} />}
			{results && hits.length === 0 && (
				<EmptyState icon={SearchX} title={t("search.noResults")} />
			)}
			{hits.length > 0 && (
				<div
					ref={list}
					id="results"
					role="listbox"
					aria-label={t("search.results")}
					className="library-results"
				>
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
			)}
			<p role="status" className="library-status">
				{status}
			</p>
		</div>
	);
}
