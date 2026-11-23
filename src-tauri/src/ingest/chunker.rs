// SPDX-License-Identifier: GPL-3.0-or-later
//! `ParsedDocument` -> `Vec<Chunk>`. Pure: no I/O.
//!
//! Offsets are `char` indices into `doc_text(doc)`. A chunk never spans blocks (a page, a
//! section, a run of lines, an image). Within a block, text is cut into atoms by paragraph,
//! then sentence, then word, then raw chars, and atoms are packed greedily up to the target.

use sha2::{Digest, Sha256};
use unicode_segmentation::UnicodeSegmentation;

use super::parser::{Locator, ParsedDocument};
use crate::storage::db::DocId;

pub struct ChunkerConfig {
    pub target_tokens: usize,
    pub max_tokens: usize,
    pub overlap_tokens: usize,
}

impl Default for ChunkerConfig {
    fn default() -> Self {
        Self {
            target_tokens: 400,
            max_tokens: 512,
            overlap_tokens: 60,
        }
    }
}

pub trait TokenCounter: Send + Sync {
    fn count(&self, text: &str) -> usize;
}

/// `ceil(chars / 4)`. Tests only: the app counts with the embedding model's tokenizer
/// (`rag::embedder::BertEmbedder`), and chars/4 undercounts Hangul and CJK.
#[cfg(test)]
pub struct CharApprox;

#[cfg(test)]
impl TokenCounter for CharApprox {
    fn count(&self, text: &str) -> usize {
        text.chars().count().div_ceil(4)
    }
}

// No Debug: holds document text.
#[derive(Clone, PartialEq)]
pub struct Chunk {
    pub doc_id: DocId,
    pub chunk_index: u32,
    pub page: Option<u32>,
    pub section_path: Option<String>,
    /// `char` index into `doc_text`, inclusive.
    pub start_offset: u32,
    /// `char` index into `doc_text`, exclusive.
    pub end_offset: u32,
    /// At most 8 rects, `[x0, y0, x1, y1]` normalized, top-left origin. `None` without spans.
    pub bbox: Option<Vec<[f32; 4]>>,
    pub text: String,
    pub sha256: [u8; 32],
}

/// The units' text joined with `\n`: the string chunk offsets point into.
pub fn doc_text(doc: &ParsedDocument) -> String {
    let texts: Vec<&str> = doc.units.iter().map(|u| u.text.as_str()).collect();
    texts.join("\n")
}

const MAX_RECTS: usize = 8;

/// A char-indexed view of the doc text.
struct Text<'a> {
    s: &'a str,
    chars: Vec<char>,
    /// Byte offset of each char, plus `s.len()` at the end.
    at: Vec<usize>,
    counter: &'a dyn TokenCounter,
}

impl Text<'_> {
    fn slice(&self, a: usize, b: usize) -> &str {
        &self.s[self.at[a]..self.at[b]]
    }

    fn count(&self, a: usize, b: usize) -> usize {
        self.counter.count(self.slice(a, b))
    }

    fn ws(&self, i: usize) -> bool {
        self.chars[i].is_whitespace()
    }

    /// Char index of a byte offset inside the slice that starts at char `a`.
    fn char_at(&self, a: usize, byte: usize) -> usize {
        let abs = self.at[a] + byte;
        self.at.binary_search(&abs).unwrap_or_else(|i| i)
    }

    fn trim(&self, mut a: usize, mut b: usize) -> (usize, usize) {
        while a < b && self.ws(a) {
            a += 1;
        }
        while b > a && self.ws(b - 1) {
            b -= 1;
        }
        (a, b)
    }

    /// Cuts `a..b` at `cuts`. Each cut moves back over whitespace, so whitespace leads the
    /// next piece and every piece but the last ends on a non-whitespace char.
    fn pieces(
        &self,
        a: usize,
        b: usize,
        cuts: impl IntoIterator<Item = usize>,
    ) -> Vec<(usize, usize)> {
        let mut cuts: Vec<usize> = cuts
            .into_iter()
            .map(|mut p| {
                while p > a && self.ws(p - 1) {
                    p -= 1;
                }
                p
            })
            .filter(|&p| p > a && p < b)
            .collect();
        cuts.sort_unstable();
        cuts.dedup();
        let mut out = Vec::with_capacity(cuts.len() + 1);
        let mut start = a;
        for p in cuts {
            out.push((start, p));
            start = p;
        }
        out.push((start, b));
        out
    }

    /// Pushes `(start, end, tokens)` atoms of at most `limit` tokens. Levels: 1 sentence, 2 word, 3 char.
    fn split(
        &self,
        a: usize,
        b: usize,
        level: u8,
        limit: usize,
        out: &mut Vec<(usize, usize, usize)>,
    ) {
        let n = self.count(a, b);
        if n <= limit {
            out.push((a, b, n));
            return;
        }
        let s = self.slice(a, b);
        let cuts: Vec<usize> = match level {
            1 => s
                .split_sentence_bound_indices()
                .map(|(i, _)| self.char_at(a, i))
                .collect(),
            2 => s
                .split_word_bound_indices()
                .map(|(i, _)| self.char_at(a, i))
                .collect(),
            _ => {
                let mut a = a;
                while a < b {
                    // Largest end that fits, at least one char.
                    let (mut lo, mut hi) = (a + 1, b);
                    while lo < hi {
                        let mid = (lo + hi).div_ceil(2);
                        if self.count(a, mid) <= limit {
                            lo = mid;
                        } else {
                            hi = mid - 1;
                        }
                    }
                    out.push((a, lo, self.count(a, lo)));
                    a = lo;
                }
                return;
            }
        };
        for (pa, pb) in self.pieces(a, b, cuts) {
            self.split(pa, pb, level + 1, limit, out);
        }
    }

    /// Where the next chunk starts inside the previous chunk `ps..pe`, or `None` for no overlap.
    fn overlap(&self, ps: usize, pe: usize, ov: usize) -> Option<usize> {
        // Tiny chunks don't overlap: a chunk under 2 x overlap would mostly repeat itself.
        if ov == 0 || self.count(ps, pe) < 2 * ov {
            return None;
        }
        let (lo, hi) = (ov as f64 * 0.8, ov as f64 * 1.2);
        // Latest start whose suffix count is in [lo, hi]; counts only grow going back.
        let pick = |starts: Vec<usize>| {
            for &p in starts.iter().rev() {
                let c = self.count(p, pe) as f64;
                if c > hi {
                    break;
                }
                if c >= lo {
                    return Some(p);
                }
            }
            None
        };
        let s = self.slice(ps, pe);
        let starts = |idx: Vec<usize>| -> Vec<usize> {
            idx.into_iter()
                .map(|i| self.char_at(ps, i))
                .filter(|&p| p > ps && !self.ws(p))
                .collect()
        };
        pick(starts(
            s.split_sentence_bound_indices().map(|(i, _)| i).collect(),
        ))
        .or_else(|| {
            pick(starts(
                s.split_word_bound_indices().map(|(i, _)| i).collect(),
            ))
        })
        .or_else(|| {
            // Largest start whose suffix still holds `ov` tokens.
            let (mut lo, mut hi) = (ps + 1, pe - 1);
            while lo < hi {
                let mid = (lo + hi).div_ceil(2);
                if self.count(mid, pe) >= ov {
                    lo = mid;
                } else {
                    hi = mid - 1;
                }
            }
            while lo > ps + 1 && self.ws(lo) {
                lo -= 1;
            }
            Some(lo)
        })
    }
}

#[derive(PartialEq)]
enum Key<'a> {
    Page(u32),
    Section(&'a [String]),
    Line,
    Image,
}

fn key(l: &Locator) -> Key<'_> {
    match l {
        Locator::Page(n) => Key::Page(*n),
        Locator::Section { heading_path } => Key::Section(heading_path),
        Locator::Line(_) => Key::Line,
        Locator::Image => Key::Image,
    }
}

pub fn chunk(
    doc_id: DocId,
    doc: &ParsedDocument,
    cfg: &ChunkerConfig,
    counter: &dyn TokenCounter,
) -> Vec<Chunk> {
    let s = doc_text(doc);
    let mut t = Text {
        s: &s,
        chars: Vec::new(),
        at: Vec::new(),
        counter,
    };
    for (i, c) in s.char_indices() {
        t.chars.push(c);
        t.at.push(i);
    }
    t.at.push(s.len());

    let mut bases = Vec::with_capacity(doc.units.len());
    let mut pos = 0;
    for u in &doc.units {
        bases.push(pos);
        pos += u.text.chars().count() + 1;
    }
    let end_of = |u: usize| bases[u] + doc.units[u].text.chars().count();
    // Overlap (at most 1.2 x overlap) plus one atom always fits in max_tokens.
    let limit = cfg
        .max_tokens
        .saturating_sub((cfg.overlap_tokens * 6).div_ceil(5))
        .max(1);

    let mut out = Vec::new();
    let mut lo = 0;
    while lo < doc.units.len() {
        let k = key(&doc.units[lo].locator);
        let mut hi = lo + 1;
        while hi < doc.units.len() && key(&doc.units[hi].locator) == k {
            hi += 1;
        }
        let (bs, be) = (bases[lo], end_of(hi - 1));

        // Paragraphs: each unit, and blank lines inside a unit.
        let mut cuts: Vec<usize> = bases[lo + 1..hi].to_vec();
        cuts.extend(
            (bs..be.saturating_sub(1)).filter(|&i| t.chars[i] == '\n' && t.chars[i + 1] == '\n'),
        );
        let mut atoms = Vec::new();
        for (a, b) in t.pieces(bs, be, cuts) {
            t.split(a, b, 1, limit, &mut atoms);
        }

        let (page, section_path) = match &k {
            Key::Page(n) => (Some(*n), None),
            Key::Section(p) if !p.is_empty() => (None, Some(p.join(" > "))),
            _ => (None, None),
        };
        let mut prev: Option<(usize, usize)> = None;
        let mut i = 0;
        while i < atoms.len() {
            let mut start = atoms[i].0;
            let mut total = 0;
            if let Some(s) = prev.and_then(|(ps, pe)| t.overlap(ps, pe, cfg.overlap_tokens)) {
                start = s;
                total = t.count(s, atoms[i].0);
            }
            let mut j = i;
            while j < atoms.len() && (j == i || total + atoms[j].2 <= cfg.target_tokens) {
                total += atoms[j].2;
                j += 1;
            }
            // Sums bound CharApprox; other counters may not be subadditive.
            while j > i + 1 && t.count(start, atoms[j - 1].1) > cfg.max_tokens {
                j -= 1;
            }
            if t.count(start, atoms[j - 1].1) > cfg.max_tokens {
                start = atoms[i].0;
            }
            let (a, b) = t.trim(start, atoms[j - 1].1);
            i = j;
            if a == b {
                continue;
            }
            let text = t.slice(a, b).to_owned();
            out.push(Chunk {
                doc_id,
                chunk_index: u32::try_from(out.len()).unwrap_or(u32::MAX),
                page,
                section_path: section_path.clone(),
                start_offset: u32::try_from(a).unwrap_or(u32::MAX),
                end_offset: u32::try_from(b).unwrap_or(u32::MAX),
                bbox: bbox(doc, &bases, lo..hi, a, b),
                sha256: Sha256::digest(text.as_bytes()).into(),
                text,
            });
            prev = Some((a, b));
        }
        lo = hi;
    }
    out
}

fn union(a: [f32; 4], b: [f32; 4]) -> [f32; 4] {
    [
        a[0].min(b[0]),
        a[1].min(b[1]),
        a[2].max(b[2]),
        a[3].max(b[3]),
    ]
}

/// One rect per line of the spans in `a..b`, merged into at most `MAX_RECTS` groups.
fn bbox(
    doc: &ParsedDocument,
    bases: &[usize],
    units: std::ops::Range<usize>,
    a: usize,
    b: usize,
) -> Option<Vec<[f32; 4]>> {
    let mut lines: Vec<[f32; 4]> = Vec::new();
    for u in units {
        for sp in &doc.units[u].spans {
            let (s, e) = (
                bases[u] + sp.char_start as usize,
                bases[u] + sp.char_end as usize,
            );
            if s >= b || e <= a {
                continue;
            }
            let cy = (sp.bbox[1] + sp.bbox[3]) / 2.0;
            match lines.last_mut() {
                Some(l) if cy >= l[1] && cy <= l[3] => *l = union(*l, sp.bbox),
                _ => lines.push(sp.bbox),
            }
        }
    }
    if lines.is_empty() {
        return None;
    }
    let per = lines.len().div_ceil(MAX_RECTS);
    Some(
        lines
            .chunks(per)
            .map(|g| g[1..].iter().fold(g[0], |r, l| union(r, *l)))
            .collect(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ingest::parser::{DocKind, DocMeta, Span, TextUnit};
    use proptest::prelude::*;

    fn doc(units: Vec<TextUnit>) -> ParsedDocument {
        ParsedDocument {
            kind: DocKind::Txt,
            units,
            meta: DocMeta {
                title: None,
                created: None,
                modified: None,
            },
            pages: Vec::new(),
        }
    }

    fn unit(locator: Locator, text: &str) -> TextUnit {
        TextUnit {
            locator,
            text: text.to_owned(),
            spans: Vec::new(),
        }
    }

    fn run(d: &ParsedDocument) -> Vec<Chunk> {
        chunk(DocId(7), d, &ChunkerConfig::default(), &CharApprox)
    }

    fn slice(chars: &[char], a: u32, b: u32) -> String {
        chars[a as usize..b as usize].iter().collect()
    }

    /// Text, coverage, and limits that hold for every document.
    fn check_basic(d: &ParsedDocument, chunks: &[Chunk]) {
        let text = doc_text(d);
        let chars: Vec<char> = text.chars().collect();
        let mut covered = vec![false; chars.len()];
        for (i, c) in chunks.iter().enumerate() {
            assert_eq!(c.chunk_index as usize, i);
            assert!(CharApprox.count(&c.text) <= 512);
            assert_eq!(c.text, slice(&chars, c.start_offset, c.end_offset));
            assert_eq!(
                c.sha256,
                <[u8; 32]>::from(Sha256::digest(c.text.as_bytes()))
            );
            assert!(c.start_offset < c.end_offset);
            if i > 0 {
                assert!(chunks[i - 1].start_offset < c.start_offset);
            }
            covered[c.start_offset as usize..c.end_offset as usize].fill(true);
        }
        for (i, ch) in chars.iter().enumerate() {
            assert!(covered[i] || ch.is_whitespace(), "char {i} not covered");
        }
        let first = chars.iter().position(|c| !c.is_whitespace());
        let last = chars.iter().rposition(|c| !c.is_whitespace());
        assert_eq!(chunks.first().map(|c| c.start_offset as usize), first);
        let max_end = chunks.iter().map(|c| c.end_offset as usize).max();
        assert_eq!(max_end, last.map(|l| l + 1));
    }

    #[test]
    fn empty_and_whitespace_docs_have_no_chunks() {
        assert!(run(&doc(vec![])).is_empty());
        assert!(run(&doc(vec![unit(Locator::Line(1), " \n\n \t")])).is_empty());
    }

    #[test]
    fn one_char_doc_is_one_chunk() {
        let d = doc(vec![unit(Locator::Line(1), "a")]);
        let c = run(&d);
        assert_eq!(c.len(), 1);
        assert_eq!(
            (c[0].start_offset, c[0].end_offset, c[0].text.as_str()),
            (0, 1, "a")
        );
        assert!(c[0].bbox.is_none());
    }

    #[test]
    fn long_paragraph_is_split_by_sentences() {
        let text = "The quick brown fox jumps over the lazy dog again. ".repeat(400);
        let d = doc(vec![unit(Locator::Line(1), text.trim_end())]);
        let c = run(&d);
        assert!(c.len() > 10);
        check_basic(&d, &c);
        for w in c.windows(2) {
            assert!(
                w[1].start_offset < w[0].end_offset,
                "consecutive chunks overlap"
            );
        }
        assert!(c.iter().all(|c| c.text.starts_with("The")));
    }

    #[test]
    fn single_long_word_is_split_by_chars() {
        let d = doc(vec![unit(Locator::Line(1), &"x".repeat(10_000))]);
        let c = run(&d);
        check_basic(&d, &c);
        assert!(c.len() >= 10_000 / (512 * 4));
    }

    #[test]
    fn sections_and_pages_label_chunks() {
        let path = vec!["A".to_owned(), "B".to_owned()];
        let d = doc(vec![
            unit(Locator::Section { heading_path: path }, "one"),
            unit(
                Locator::Section {
                    heading_path: vec![],
                },
                "two",
            ),
            unit(Locator::Page(3), "three"),
        ]);
        let c = run(&d);
        let labels: Vec<_> = c.iter().map(|c| (c.page, c.section_path.clone())).collect();
        assert_eq!(
            labels,
            vec![
                (None, Some("A > B".to_owned())),
                (None, None),
                (Some(3), None)
            ]
        );
    }

    #[test]
    fn bbox_merges_lines_into_at_most_eight() {
        // 20 lines of one word each, two words per line.
        let mut text = String::new();
        let mut spans = Vec::new();
        for l in 0..20u32 {
            for w in 0..2u32 {
                let s = text.chars().count() as u32;
                text.push_str("word ");
                let y = l as f32 / 25.0;
                let x = w as f32 / 4.0;
                spans.push(Span {
                    char_start: s,
                    char_end: s + 4,
                    bbox: [x, y, x + 0.2, y + 0.03],
                });
            }
            text.push('\n');
        }
        let d = doc(vec![TextUnit {
            locator: Locator::Page(1),
            text,
            spans,
        }]);
        let c = run(&d);
        assert_eq!(c.len(), 1);
        let rects = c[0].bbox.clone().unwrap();
        assert_eq!(rects.len(), 7); // 20 lines, 3 per group
        assert_eq!(rects[0], [0.0, 0.0, 0.45, 2.0 / 25.0 + 0.03]);
    }

    // Generated documents: tokens carry their own separators.
    fn token() -> impl Strategy<Value = String> {
        prop_oneof![
            8 => prop::sample::select(vec!["alpha", "beta", "gamma", "delta", "한국어", "문서", "검색"])
                .prop_map(|w| format!("{w} ")),
            2 => prop::sample::select(vec![". ", "? ", "。"]).prop_map(str::to_owned),
            1 => Just("\n\n".to_owned()),
            1 => Just("\n".to_owned()),
            1 => (100usize..2500).prop_map(|n| "z".repeat(n)),
        ]
    }

    /// `(kind, n, text)` specs: proptest needs Debug values, and `TextUnit` has none.
    fn specs() -> impl Strategy<Value = Vec<(u8, u32, String)>> {
        let u = (
            0u8..4,
            0u32..3,
            prop::collection::vec(token(), 0..300).prop_map(|t| t.concat()),
        );
        prop::collection::vec(u, 0..8)
    }

    fn units(specs: Vec<(u8, u32, String)>) -> Vec<TextUnit> {
        specs
            .into_iter()
            .map(|(kind, n, text)| {
                let locator = match kind {
                    0 => Locator::Page(n + 1),
                    1 => Locator::Section {
                        heading_path: (0..n).map(|i| format!("H{i}")).collect(),
                    },
                    2 => Locator::Line(n),
                    _ => Locator::Image,
                };
                let spans = if kind == 0 {
                    word_spans(&text)
                } else {
                    Vec::new()
                };
                TextUnit {
                    locator,
                    text,
                    spans,
                }
            })
            .collect()
    }

    /// One span per non-whitespace run, laid out by line.
    fn word_spans(text: &str) -> Vec<Span> {
        let mut spans = Vec::new();
        let (mut line, mut col, mut start) = (0u32, 0u32, None);
        let chars: Vec<char> = text.chars().collect();
        for i in 0..=chars.len() {
            let ws = chars.get(i).is_none_or(|c| c.is_whitespace());
            match (ws, start) {
                (false, None) => start = Some(i as u32),
                (true, Some(s)) => {
                    let y = (line % 90) as f32 / 100.0;
                    let x = (col % 50) as f32 / 60.0;
                    spans.push(Span {
                        char_start: s,
                        char_end: i as u32,
                        bbox: [x, y, x + 0.1, y + 0.009],
                    });
                    start = None;
                    col += 1;
                }
                _ => {}
            }
            if chars.get(i) == Some(&'\n') {
                line += 1;
                col = 0;
            }
        }
        spans
    }

    fn block_label(l: &Locator) -> String {
        match l {
            Locator::Page(n) => format!("p{n}"),
            Locator::Section { heading_path } => format!("s{}", heading_path.join(">")),
            Locator::Line(_) => "l".to_owned(),
            Locator::Image => "i".to_owned(),
        }
    }

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(96))]
        #[test]
        fn chunks_hold_invariants(specs in specs()) {
            let d = doc(units(specs));
            let chunks = run(&d);
            check_basic(&d, &chunks);

            // Unit index and block id of every doc char; None on separators.
            let mut unit_of = Vec::new();
            let mut block_of_unit = Vec::new();
            let mut block = 0usize;
            for (i, u) in d.units.iter().enumerate() {
                if i > 0 {
                    unit_of.push(None);
                    if block_label(&u.locator) != block_label(&d.units[i - 1].locator) {
                        block += 1;
                    }
                }
                block_of_unit.push(block);
                unit_of.extend(std::iter::repeat_n(Some(i), u.text.chars().count()));
            }
            let mut bases = Vec::new();
            let mut pos = 0;
            for u in &d.units {
                bases.push(pos);
                pos += u.text.chars().count() + 1;
            }
            let block_of = |c: &Chunk| unit_of[c.start_offset as usize].map(|u| block_of_unit[u]);

            for c in &chunks {
                let touched: Vec<usize> = unit_of[c.start_offset as usize..c.end_offset as usize]
                    .iter().flatten().copied().collect();
                for &u in &touched {
                    prop_assert_eq!(block_of_unit[u], block_of(c).unwrap());
                    let page = match d.units[u].locator { Locator::Page(n) => Some(n), _ => None };
                    prop_assert_eq!(page, c.page);
                }
                let any_span = touched.iter().any(|&u| d.units[u].spans.iter().any(|s| {
                    let (a, b) = (bases[u] + s.char_start as usize, bases[u] + s.char_end as usize);
                    a < c.end_offset as usize && b > c.start_offset as usize
                }));
                prop_assert_eq!(c.bbox.is_some(), any_span);
                if let Some(rects) = &c.bbox {
                    prop_assert!(rects.len() <= 8);
                    prop_assert!(rects.iter().flatten().all(|v| (0.0..=1.0).contains(v)));
                }
            }

            let chars: Vec<char> = doc_text(&d).chars().collect();
            for w in chunks.windows(2) {
                let (p, n) = (&w[0], &w[1]);
                if block_of(p) == block_of(n) && CharApprox.count(&p.text) >= 120 {
                    prop_assert!(n.start_offset > p.start_offset && n.start_offset < p.end_offset);
                    let ov = CharApprox.count(&slice(&chars, n.start_offset, p.end_offset));
                    prop_assert!((48..=72).contains(&ov), "overlap {} tokens", ov);
                } else {
                    prop_assert!(n.start_offset >= p.end_offset);
                }
            }
        }
    }
}
