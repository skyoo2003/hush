# Retrieval eval

`cargo run --example eval` (from `src-tauri/`) indexes `corpus/` into a temporary DB and runs `queries.jsonl` in VectorOnly and Hybrid mode. It needs the default embedding model (`scripts/fetch-models.sh`).

## queries.jsonl

One JSON object per line:

```json
{"id":"q01","query":"...","kind":"proper_noun|clause|numeric|concept|no_answer","relevant":[{"file":"gpl-3.0.txt","anchor":"exact phrase"}]}
```

A hit is relevant when its file matches and its chunk contains the anchor (case-insensitive, whitespace-normalized). `no_answer` queries have no relevant chunk and feed the top_score distribution.

## Corpus sources

All public texts, copied verbatim.

| File | Source | License |
| --- | --- | --- |
| `gpl-3.0.txt` | GNU GPL v3, this repository's `LICENSE` | Verbatim copying permitted |
| `apache-2.0.txt`, `mit.txt`, `mpl-2.0.txt`, `bsd-2-clause.txt`, `bsl-1.0.txt`, `unlicense.txt`, `cc0-1.0.txt` | License files shipped in crates on crates.io | License texts, verbatim |
| `commonpaper-mutual-nda.md`, `commonpaper-cloud-service-agreement.md`, `commonpaper-sla.md`, `commonpaper-dpa.md` | [Common Paper](https://github.com/CommonPaper) standard agreements | [CC BY 4.0](https://creativecommons.org/licenses/by/4.0/), © Common Paper, Inc. |
| `federal-source-code-policy.md` | OMB M-16-21, [WhiteHouse/source-code-policy](https://github.com/WhiteHouse/source-code-policy) `pages/*.md`, concatenated | U.S. Government work, public domain |
