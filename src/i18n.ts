// SPDX-License-Identifier: GPL-3.0-or-later
// UI strings. `{name}` placeholders are filled by `t`.

const en = {
  "search.label": "Search",
  "search.placeholder": "Search your folders",
  "search.hint":
    'Narrow with type:pdf|doc|text|image, ext:, folder:, after:2025-01, before:, or "an exact phrase".',
  "search.noResults": "No matching passages in your folders.",
  "search.indexing": "Still indexing {count} files. Results may be incomplete.",
  "search.modelMissing":
    "Search is off until the embedding model is installed.",
  "search.sourceNotFound":
    "That file was moved or deleted. It is being removed from the results.",
  "search.clipboardFailed": "Could not copy the citation.",
  "search.failed": "Something went wrong. Try again.",
  "search.copied": "Citation copied.",
  "search.results": "Results",
  "search.filters": "Filters",
  "search.keys.open": "open",
  "search.keys.reveal": "show in Finder",
  "search.keys.copy": "copy citation",
  "search.keys.close": "close",
  "filter.type": "Type",
  "filter.ext": "Extension",
  "filter.folder": "Folder",
  "filter.after": "After",
  "filter.before": "Before",
  "card.page": "p. {page}",
  "card.modified": "Modified {date}",
};

export type MessageKey = keyof typeof en;

const ko: Record<MessageKey, string> = {
  "search.label": "검색",
  "search.placeholder": "폴더에서 검색",
  "search.hint":
    'type:pdf|doc|text|image, ext:, folder:, after:2025-01, before:, "정확한 구절"로 범위를 좁힐 수 있습니다.',
  "search.noResults": "폴더에서 일치하는 구절을 찾지 못했습니다.",
  "search.indexing":
    "파일 {count}개를 아직 색인하는 중입니다. 결과가 일부만 보일 수 있습니다.",
  "search.modelMissing": "임베딩 모델을 설치해야 검색할 수 있습니다.",
  "search.sourceNotFound":
    "파일이 옮겨졌거나 삭제되었습니다. 결과에서 곧 빠집니다.",
  "search.clipboardFailed": "인용을 복사하지 못했습니다.",
  "search.failed": "문제가 생겼습니다. 다시 시도하세요.",
  "search.copied": "인용을 복사했습니다.",
  "search.results": "결과",
  "search.filters": "필터",
  "search.keys.open": "열기",
  "search.keys.reveal": "Finder에서 보기",
  "search.keys.copy": "인용 복사",
  "search.keys.close": "닫기",
  "filter.type": "유형",
  "filter.ext": "확장자",
  "filter.folder": "폴더",
  "filter.after": "이후",
  "filter.before": "이전",
  "card.page": "{page}쪽",
  "card.modified": "{date} 수정",
};

export const locale = globalThis.navigator?.language ?? "en";
const messages = locale.toLowerCase().startsWith("ko") ? ko : en;

export function t(
  key: MessageKey,
  vars: Record<string, string | number> = {},
): string {
  return messages[key].replace(/\{(\w+)\}/g, (m, name: string) =>
    String(vars[name] ?? m),
  );
}
