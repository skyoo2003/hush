// SPDX-License-Identifier: GPL-3.0-or-later
//! OCR engines behind one trait: Apple Vision on macOS, a no-op elsewhere.
//! Sync on purpose: callers run it in `spawn_blocking`.

use super::error::OcrError;
use super::parser::Span;

#[cfg(target_os = "macos")]
mod macos;
#[cfg(target_os = "macos")]
pub use macos::VisionOcr;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OcrLang {
    EnUs,
    KoKr,
}

impl OcrLang {
    pub fn bcp47(self) -> &'static str {
        match self {
            Self::EnUs => "en-US",
            Self::KoKr => "ko-KR",
        }
    }
}

/// One recognized line. No Debug: it holds document text.
#[derive(Clone, PartialEq)]
pub struct OcrLine {
    pub text: String,
    /// Same convention as `Span::bbox`: `[x0, y0, x1, y1]`, 0..1, top-left origin.
    pub bbox: [f32; 4],
}

pub trait OcrEngine: Send + Sync {
    fn recognize(
        &self,
        image: &image::RgbaImage,
        langs: &[OcrLang],
    ) -> Result<Vec<OcrLine>, OcrError>;
}

/// Recognizes nothing. For platforms without an engine, and for tests.
pub struct NoopOcr;

impl OcrEngine for NoopOcr {
    fn recognize(&self, _: &image::RgbaImage, _: &[OcrLang]) -> Result<Vec<OcrLine>, OcrError> {
        Ok(Vec::new())
    }
}

pub fn default_engine() -> Box<dyn OcrEngine> {
    #[cfg(target_os = "macos")]
    return Box::new(VisionOcr);
    #[cfg(not(target_os = "macos"))]
    return Box::new(NoopOcr);
}

/// Sorts lines top to bottom, and left to right within a row of lines whose vertical
/// centers overlap; joins a row with spaces and rows with `\n`. One `Span` per line.
// shortcut: no layout analysis, so multi-column pages and tables interleave; cluster columns/cells when has_tables pages need it.
pub fn join_lines(mut lines: Vec<OcrLine>) -> (String, Vec<Span>) {
    lines.retain(|l| !l.text.trim().is_empty());
    let center = |l: &OcrLine| (l.bbox[1] + l.bbox[3]) / 2.0;
    lines.sort_by(|a, b| center(a).total_cmp(&center(b)));

    let mut rows: Vec<Vec<OcrLine>> = Vec::new();
    for line in lines {
        match rows.last_mut() {
            Some(row) if center(&line) <= row[0].bbox[3] => row.push(line),
            _ => rows.push(vec![line]),
        }
    }

    let mut text = String::new();
    let mut spans = Vec::new();
    let mut n = 0u32;
    for (r, mut row) in rows.into_iter().enumerate() {
        row.sort_by(|a, b| a.bbox[0].total_cmp(&b.bbox[0]));
        for (i, line) in row.into_iter().enumerate() {
            if r > 0 || i > 0 {
                text.push(if i == 0 { '\n' } else { ' ' });
                n += 1;
            }
            let t = line.text.trim();
            let len = u32::try_from(t.chars().count()).unwrap_or(u32::MAX);
            text.push_str(t);
            spans.push(Span {
                char_start: n,
                char_end: n.saturating_add(len),
                bbox: line.bbox,
            });
            n = n.saturating_add(len);
        }
    }
    (text, spans)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn line(text: &str, bbox: [f32; 4]) -> OcrLine {
        OcrLine {
            text: text.into(),
            bbox,
        }
    }

    #[test]
    fn join_lines_reading_order() {
        let lines = vec![
            line("third", [0.1, 0.50, 0.4, 0.55]),
            line("right", [0.6, 0.11, 0.9, 0.16]),
            line("  ", [0.1, 0.30, 0.2, 0.35]),
            line("left 한글", [0.1, 0.10, 0.4, 0.15]),
            line("second", [0.1, 0.30, 0.4, 0.35]),
        ];
        let (text, spans) = join_lines(lines);
        assert_eq!(text, "left 한글 right\nsecond\nthird");
        let chars: Vec<char> = text.chars().collect();
        let words: Vec<String> = spans
            .iter()
            .map(|s| {
                chars[s.char_start as usize..s.char_end as usize]
                    .iter()
                    .collect()
            })
            .collect();
        assert_eq!(words, ["left 한글", "right", "second", "third"]);
        assert_eq!(spans[1].bbox, [0.6, 0.11, 0.9, 0.16]);
        assert!(join_lines(Vec::new()).0.is_empty());
    }
}
