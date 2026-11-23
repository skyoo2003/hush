// SPDX-License-Identifier: GPL-3.0-or-later
//! PDF -> `ParsedDocument` through PDFium: per-word spans, scan detection, OCR fallback.
//! Sync on purpose: callers run it in `spawn_blocking`.

use std::path::Path;
use std::sync::{Arc, Mutex, OnceLock};

use pdfium_render::prelude::*;

use super::error::ParseError;
use super::ocr::{self, OcrEngine, OcrLang};
use super::parser::{
    read_capped, DocKind, Locator, PageInfo, ParsedDocument, Parser, Span, TextUnit,
};
use crate::security::paths::ScopedPath;

pub const PDF_MAGIC: &[u8] = b"%PDF-";

/// A page with fewer non-whitespace chars than this is treated as a scan.
pub const MIN_TEXT_CHARS: usize = 30;
/// A 40-char line of 8 pt text covers about 0.15% of an A4 page; below 0.1% only a footer-sized string remains.
pub const MIN_TEXT_AREA_RATIO: f32 = 0.001;
/// A table needs at least this many horizontal and this many vertical rules.
pub const MIN_GRID_LINES: usize = 3;
/// A rule must span this fraction of the page width (horizontal) or height (vertical).
const MIN_RULE_FRACTION: f32 = 0.05;
/// Pages are rendered this wide for OCR...
const OCR_TARGET_WIDTH: f32 = 2000.0;
/// ...but never with more pixels than this (a 2000 px A4 page is about 5.7 MP).
const OCR_MAX_PIXELS: f32 = 12_000_000.0;

// PDFium's bindings are process-global: a second bind fails, so every parser shares one.
static PDFIUM: OnceLock<Pdfium> = OnceLock::new();
static BIND: Mutex<()> = Mutex::new(());

pub struct PdfParser {
    pdfium: &'static Pdfium,
    ocr: Arc<dyn OcrEngine>,
    pub langs: Vec<OcrLang>,
}

impl PdfParser {
    /// Binds PDFium from `library` on first use; later calls reuse that binding and ignore `library`.
    pub fn new(library: &Path, ocr: Arc<dyn OcrEngine>) -> Result<Self, ParseError> {
        let pdfium = match PDFIUM.get() {
            Some(p) => p,
            None => {
                let _guard = BIND.lock().map_err(|_| ParseError::PdfiumUnavailable)?;
                match PDFIUM.get() {
                    Some(p) => p,
                    None => {
                        let bindings = Pdfium::bind_to_library(library)?;
                        PDFIUM.get_or_init(|| Pdfium::new(bindings))
                    }
                }
            }
        };
        Ok(Self {
            pdfium,
            ocr,
            langs: vec![OcrLang::KoKr, OcrLang::EnUs],
        })
    }
}

impl Parser for PdfParser {
    fn supports(&self, ext: &str) -> bool {
        ext == "pdf"
    }

    fn parse(&self, path: &ScopedPath) -> Result<ParsedDocument, ParseError> {
        let (bytes, mut meta) = read_capped(path)?;
        let doc = self.pdfium.load_pdf_from_byte_vec(bytes, None)?;
        meta.title = doc
            .metadata()
            .get(PdfDocumentMetadataTagType::Title)
            .map(|t| t.value().trim().to_owned())
            .filter(|t| !t.is_empty());

        let mut units = Vec::new();
        let mut pages = Vec::new();
        for (i, page) in doc.pages().iter().enumerate() {
            let number = u32::try_from(i + 1).unwrap_or(u32::MAX);
            let (w, h) = (page.width().value, page.height().value);
            let (mut text, mut spans) = text_layer(&page, w, h)?;
            let needs_ocr = needs_ocr(&text, &spans);
            if needs_ocr {
                let image = render(&page, w, h)?;
                // One page's OCR failure keeps that page's text layer instead of dropping the document.
                match self.ocr.recognize(&image, &self.langs) {
                    Ok(lines) => {
                        let (t, s) = ocr::join_lines(lines);
                        if non_ws(&t) > non_ws(&text) {
                            (text, spans) = (t, s);
                        }
                    }
                    Err(e) => {
                        tracing::warn!(page = number, error = %e, "OCR failed; keeping text layer")
                    }
                }
            }
            pages.push(PageInfo {
                number,
                needs_ocr,
                has_tables: has_grid(&page, w, h),
            });
            if !text.trim().is_empty() {
                units.push(TextUnit {
                    locator: Locator::Page(number),
                    text,
                    spans,
                });
            }
        }
        Ok(ParsedDocument {
            kind: DocKind::Pdf,
            units,
            meta,
            pages,
        })
    }
}

fn non_ws(s: &str) -> usize {
    s.chars().filter(|c| !c.is_whitespace()).count()
}

fn needs_ocr(text: &str, spans: &[Span]) -> bool {
    let area: f32 = spans
        .iter()
        .map(|s| (s.bbox[2] - s.bbox[0]) * (s.bbox[3] - s.bbox[1]))
        .sum();
    non_ws(text) < MIN_TEXT_CHARS || area < MIN_TEXT_AREA_RATIO
}

/// PDF points (bottom-left origin) -> normalized top-left `[x0, y0, x1, y1]`.
// shortcut: assumes an unrotated page whose box starts at (0, 0); handle /Rotate and offset boxes when such PDFs show up.
fn normalize(r: &PdfRect, w: f32, h: f32) -> [f32; 4] {
    [
        r.left().value / w,
        1.0 - r.top().value / h,
        r.right().value / w,
        1.0 - r.bottom().value / h,
    ]
    .map(|v| v.clamp(0.0, 1.0))
}

/// The page's text with `\r\n` folded to `\n`, and one span per maximal non-whitespace run.
fn text_layer(page: &PdfPage, w: f32, h: f32) -> Result<(String, Vec<Span>), ParseError> {
    let page_text = page.text()?;
    let page_chars = page_text.chars();
    let chars: Vec<_> = page_chars
        .iter()
        .filter_map(|c| Some((c.unicode_char()?, c)))
        .collect();

    let mut text = String::new();
    let mut spans = Vec::new();
    let mut n = 0u32;
    let mut word: Option<(u32, Option<[f32; 4]>)> = None;
    let mut close = |word: &mut Option<(u32, Option<[f32; 4]>)>, end: u32| {
        if let Some((start, Some(bbox))) = word.take() {
            spans.push(Span {
                char_start: start,
                char_end: end,
                bbox,
            });
        }
    };
    for (i, (ch, c)) in chars.iter().enumerate() {
        let ch = match ch {
            '\r' if chars.get(i + 1).is_some_and(|(next, _)| *next == '\n') => continue,
            '\r' => '\n',
            ch => *ch,
        };
        if ch.is_whitespace() {
            close(&mut word, n);
        } else {
            let start = word.get_or_insert((n, None));
            if let Ok(r) = c.tight_bounds() {
                let b = normalize(&r, w, h);
                start.1 = Some(match start.1 {
                    Some(a) => [
                        a[0].min(b[0]),
                        a[1].min(b[1]),
                        a[2].max(b[2]),
                        a[3].max(b[3]),
                    ],
                    None => b,
                });
            }
        }
        text.push(ch);
        n += 1;
    }
    close(&mut word, n);
    Ok((text, spans))
}

/// At least `MIN_GRID_LINES` long horizontal and vertical rules among the page's path objects.
// shortcut: form XObjects are not walked, so tables drawn inside one are missed; recurse into them if that shows up.
fn has_grid(page: &PdfPage, w: f32, h: f32) -> bool {
    let (mut horizontal, mut vertical) = (0, 0);
    for object in page.objects().iter() {
        let Some(path) = object.as_path_object() else {
            continue;
        };
        let segments = match path.matrix() {
            Ok(m) => path.segments().transform(m),
            Err(_) => path.segments(),
        };
        let mut start = (0.0, 0.0);
        let mut prev = (0.0, 0.0);
        let mut rule = |a: (f32, f32), b: (f32, f32)| {
            let (dx, dy) = ((b.0 - a.0).abs(), (b.1 - a.1).abs());
            if dy < 1.0 && dx > w * MIN_RULE_FRACTION {
                horizontal += 1;
            } else if dx < 1.0 && dy > h * MIN_RULE_FRACTION {
                vertical += 1;
            }
        };
        for seg in segments.iter() {
            let p = (seg.x().value, seg.y().value);
            match seg.segment_type() {
                PdfPathSegmentType::MoveTo => start = p,
                PdfPathSegmentType::LineTo => rule(prev, p),
                _ => {}
            }
            if seg.is_close() {
                rule(p, start);
            }
            prev = p;
        }
    }
    horizontal >= MIN_GRID_LINES && vertical >= MIN_GRID_LINES
}

/// Renders the page into memory, about `OCR_TARGET_WIDTH` wide and at most `OCR_MAX_PIXELS`.
fn render(page: &PdfPage, w: f32, h: f32) -> Result<image::RgbaImage, ParseError> {
    if !(w > 0.0 && h > 0.0) {
        return Err(ParseError::Corrupt("PDF page has no size"));
    }
    let (mut pw, mut ph) = (OCR_TARGET_WIDTH, OCR_TARGET_WIDTH * h / w);
    if pw * ph > OCR_MAX_PIXELS {
        let k = (OCR_MAX_PIXELS / (pw * ph)).sqrt();
        (pw, ph) = (pw * k, ph * k);
    }
    let config = PdfRenderConfig::new().set_target_size(pw.max(1.0) as i32, ph.max(1.0) as i32);
    Ok(page.render_with_config(&config)?.as_image()?.into_rgba8())
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::ingest::ocr::NoopOcr;
    use crate::ingest::parser::tests::fixture;
    use crate::ingest::parser::ParserRegistry;

    const DYLIB: &str = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/vendor/pdfium/lib/libpdfium.dylib"
    );
    const LINE: &str = "The quick brown fox jumps over the lazy dog";

    pub(crate) fn parser(ocr: Arc<dyn OcrEngine>) -> PdfParser {
        assert!(Path::new(DYLIB).exists(), "run scripts/fetch-pdfium.sh");
        PdfParser::new(Path::new(DYLIB), ocr).unwrap()
    }

    fn pt(v: f32) -> PdfPoints {
        PdfPoints::new(v)
    }

    type PageFn<'a> = dyn Fn(&mut PdfDocument, &mut PdfPage) + 'a;

    /// One A4 page per closure call; returns the saved PDF bytes.
    fn build(pages: &[&PageFn]) -> Vec<u8> {
        let pdfium = parser(Arc::new(NoopOcr)).pdfium;
        let mut doc = pdfium.create_new_pdf().unwrap();
        for f in pages {
            let mut page = doc
                .pages_mut()
                .create_page_at_end(PdfPagePaperSize::a4())
                .unwrap();
            f(&mut doc, &mut page);
        }
        doc.save_to_bytes().unwrap()
    }

    fn text(doc: &mut PdfDocument, page: &mut PdfPage, x: f32, y: f32, s: &str, size: f32) {
        let font = doc.fonts_mut().helvetica();
        page.objects_mut()
            .create_text_object(pt(x), pt(y), s, font, pt(size))
            .unwrap();
    }

    fn three_pages() -> Vec<u8> {
        build(&[
            &|d, p| text(d, p, 72.0, 760.0, &format!("Page one. {LINE}"), 12.0),
            &|d, p| {
                text(d, p, 72.0, 760.0, &format!("Page two. {LINE}"), 12.0);
                text(d, p, 100.0, 600.0, "Quartz", 24.0);
            },
            // One 40-char line of 10 pt text must not look like a scan.
            &|d, p| text(d, p, 72.0, 760.0, &LINE[..40], 10.0),
        ])
    }

    fn parse_with(bytes: &[u8], ocr: Arc<dyn OcrEngine>) -> ParsedDocument {
        let (_d, path) = fixture(bytes, "doc.pdf");
        ParserRegistry::new()
            .with_parser(Some(PDF_MAGIC), parser(ocr))
            .parse(&path)
            .unwrap()
    }

    fn slice(text: &str, s: &Span) -> String {
        text.chars()
            .skip(s.char_start as usize)
            .take((s.char_end - s.char_start) as usize)
            .collect()
    }

    #[test]
    fn pdf_text_layer_pages_and_word_spans() {
        let doc = parse_with(&three_pages(), Arc::new(NoopOcr));
        assert!(doc.kind == DocKind::Pdf);
        let locators: Vec<_> = doc.units.iter().map(|u| u.locator.clone()).collect();
        assert!(locators == [Locator::Page(1), Locator::Page(2), Locator::Page(3)]);
        assert!(doc.units[0].text.contains("Page one. The quick brown fox"));
        assert_eq!(doc.pages.len(), 3);
        assert!(doc.pages.iter().all(|p| !p.needs_ocr && !p.has_tables));
        assert_eq!(doc.pages[1].number, 2);

        let unit = &doc.units[1];
        assert!(!unit.text.contains('\r'));
        let word: Vec<_> = unit
            .spans
            .iter()
            .filter(|s| slice(&unit.text, s) == "Quartz")
            .collect();
        assert_eq!(word.len(), 1);
        // Placed at x = 100 pt, baseline y = 600 pt, 24 pt Helvetica (~72 pt wide), on 595 x 842 pt.
        let [x0, y0, x1, y1] = word[0].bbox;
        assert!((0.16..0.18).contains(&x0), "x0 {x0}");
        assert!((0.27..0.31).contains(&x1), "x1 {x1}");
        assert!((0.26..0.29).contains(&y1), "y1 {y1}");
        assert!((0.24..y1).contains(&y0), "y0 {y0}");
        for unit in &doc.units {
            assert!(unit
                .spans
                .iter()
                .all(|s| s.char_end <= unit.text.chars().count() as u32));
        }
    }

    fn image_page(page: &mut PdfPage, img: &image::RgbaImage) {
        let dynamic = image::DynamicImage::ImageRgba8(img.clone());
        let w = page.width().value - 40.0;
        let h = w * img.height() as f32 / img.width() as f32;
        page.objects_mut()
            .create_image_object(pt(20.0), pt(500.0), &dynamic, Some(pt(w)), Some(pt(h)))
            .unwrap();
    }

    #[test]
    fn pdf_scanned_page_needs_ocr() {
        let gray = image::RgbaImage::from_pixel(200, 100, image::Rgba([180, 180, 180, 255]));
        let bytes = build(&[&|_, p| image_page(p, &gray)]);
        let doc = parse_with(&bytes, Arc::new(NoopOcr));
        assert!(doc.units.is_empty());
        assert!(
            doc.pages
                == [PageInfo {
                    number: 1,
                    needs_ocr: true,
                    has_tables: false
                }]
        );
    }

    struct FailingOcr;
    impl OcrEngine for FailingOcr {
        fn recognize(
            &self,
            _: &image::RgbaImage,
            _: &[OcrLang],
        ) -> Result<Vec<ocr::OcrLine>, crate::ingest::error::OcrError> {
            Err(crate::ingest::error::OcrError::Engine("test"))
        }
    }

    #[test]
    fn pdf_ocr_failure_keeps_document() {
        let gray = image::RgbaImage::from_pixel(200, 100, image::Rgba([180, 180, 180, 255]));
        let bytes = build(&[&|_, p| image_page(p, &gray)]);
        let doc = parse_with(&bytes, Arc::new(FailingOcr));
        assert!(doc.units.is_empty());
        assert!(doc.pages.len() == 1 && doc.pages[0].needs_ocr);
    }

    #[test]
    fn pdf_grid_lines_set_has_tables() {
        let bytes = build(&[&|d, p| {
            text(
                d,
                p,
                80.0,
                760.0,
                "Quarterly figures by region and product",
                12.0,
            );
            for i in 0..5 {
                let o = 100.0 * i as f32;
                let black = PdfColor::BLACK;
                let objs = p.objects_mut();
                objs.create_path_object_line(
                    pt(80.0),
                    pt(700.0 - o),
                    pt(480.0),
                    pt(700.0 - o),
                    black,
                    pt(1.0),
                )
                .unwrap();
                objs.create_path_object_line(
                    pt(80.0 + o),
                    pt(300.0),
                    pt(80.0 + o),
                    pt(700.0),
                    black,
                    pt(1.0),
                )
                .unwrap();
            }
        }]);
        let doc = parse_with(&bytes, Arc::new(NoopOcr));
        assert!(doc.pages[0].has_tables && !doc.pages[0].needs_ocr);
    }

    #[test]
    fn pdf_garbage_is_an_error() {
        let (_d, path) = fixture(b"%PDF-1.7\n garbage", "g.pdf");
        assert!(parser(Arc::new(NoopOcr)).parse(&path).is_err());
        let (_d, path) = fixture(b"not a pdf", "n.pdf");
        let registry =
            ParserRegistry::new().with_parser(Some(PDF_MAGIC), parser(Arc::new(NoopOcr)));
        assert!(matches!(
            registry.parse(&path),
            Err(ParseError::MagicMismatch)
        ));
    }

    /// "Hello <APP_NAME> 2026" in 48 pt Helvetica, rendered by PDFium.
    pub(crate) fn hello_text() -> String {
        format!("Hello {} 2026", crate::brand::APP_NAME)
    }

    pub(crate) fn hello_image() -> image::RgbaImage {
        let bytes = build(&[&|d, p| text(d, p, 40.0, 700.0, &hello_text(), 48.0)]);
        let pdfium = parser(Arc::new(NoopOcr)).pdfium;
        let doc = pdfium.load_pdf_from_byte_vec(bytes, None).unwrap();
        let page = doc.pages().get(0).unwrap();
        let full = render(&page, page.width().value, page.height().value).unwrap();
        // The text's band only, so the image stays small.
        image::imageops::crop_imm(&full, 0, 280, full.width(), 300).to_image()
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn vision_recognizes_rendered_text() {
        use crate::ingest::ocr::VisionOcr;
        let lines = VisionOcr
            .recognize(&hello_image(), &[OcrLang::EnUs])
            .unwrap();
        let (joined, _) = ocr::join_lines(lines.clone());
        assert!(joined.contains(&hello_text()));
        for l in &lines {
            let [x0, y0, x1, y1] = l.bbox;
            assert!(l.bbox.iter().all(|v| (0.0..=1.0).contains(v)));
            assert!(x0 < x1 && y0 < y1);
        }
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn pdf_scanned_page_falls_back_to_vision() {
        use crate::ingest::ocr::VisionOcr;
        let hello = hello_image();
        let bytes = build(&[&|_, p| image_page(p, &hello)]);
        let doc = parse_with(&bytes, Arc::new(VisionOcr));
        assert!(doc.pages[0].needs_ocr);
        assert_eq!(doc.units.len(), 1);
        assert!(doc.units[0].locator == Locator::Page(1));
        assert!(doc.units[0].text.contains(crate::brand::APP_NAME));
        assert!(!doc.units[0].spans.is_empty());
    }
}
