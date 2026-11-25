// SPDX-License-Identifier: GPL-3.0-or-later
import { locale, t } from "../i18n";
import type { SearchHit } from "../ipc/SearchHit";
import { splitHighlights } from "../search";

const ICONS: Record<string, string> = {
  pdf: "📕",
  docx: "📘",
  png: "🖼️",
  jpg: "🖼️",
  jpeg: "🖼️",
};

const dateFormat = new Intl.DateTimeFormat(locale, { dateStyle: "medium" });

type Props = {
  hit: SearchHit;
  id: string;
  selected: boolean;
  onOpen: () => void;
  onHover: () => void;
};

export function SearchResultCard({
  hit,
  id,
  selected,
  onOpen,
  onHover,
}: Props) {
  const ext = hit.fileName.split(".").pop()?.toLowerCase() ?? "";
  const where =
    hit.page !== null ? t("card.page", { page: hit.page }) : hit.sectionPath;
  return (
    // Keys go through the search input (aria-activedescendant); a click opens the source.
    // biome-ignore lint/a11y/useKeyWithClickEvents: keyboard handled by the search input
    <div
      id={id}
      role="option"
      aria-selected={selected}
      tabIndex={-1}
      onClick={onOpen}
      onMouseEnter={onHover}
    >
      <span aria-hidden="true">{ICONS[ext] ?? "📄"}</span>{" "}
      <strong>{hit.fileName}</strong>
      {where && <> · {where}</>}
      <br />
      <small>
        {hit.folderLabel}
        {hit.modified !== null && (
          <>
            {" "}
            · {t("card.modified", { date: dateFormat.format(hit.modified) })}
          </>
        )}
      </small>
      <p data-selectable>
        {splitHighlights(hit.snippet, hit.highlights).map((s, i) => {
          const Tag = s.match ? "mark" : "span";
          // biome-ignore lint/suspicious/noArrayIndexKey: segments are static per render
          return <Tag key={i}>{s.text}</Tag>;
        })}
      </p>
    </div>
  );
}
