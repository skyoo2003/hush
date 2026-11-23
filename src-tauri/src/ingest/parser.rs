// SPDX-License-Identifier: GPL-3.0-or-later
//! File -> `ParsedDocument`: the registry, and the TXT, Markdown, and DOCX parsers.
//! Sync on purpose: callers run it in `spawn_blocking`.

use std::fs::File;
use std::io::{Cursor, Read, Seek};
use std::time::SystemTime;

use encoding_rs::{Encoding, EUC_KR, UTF_16BE, UTF_16LE, WINDOWS_1252};
use pulldown_cmark::{Event as MdEvent, HeadingLevel, Options, Tag, TagEnd};
use quick_xml::events::{BytesStart, Event as XmlEvent};
use quick_xml::{Reader, XmlVersion};
use zip::result::ZipError;
use zip::ZipArchive;

use super::error::ParseError;
use crate::security::paths::ScopedPath;

/// Largest file any parser reads.
pub const MAX_FILE_BYTES: u64 = 200 * 1024 * 1024;
const ZIP_MAGIC: &[u8] = b"PK\x03\x04";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DocKind {
    Txt,
    Markdown,
    Docx { has_tables: bool },
    Pdf,
    Image,
}

// No Debug on types that hold document text: it must not reach logs.
#[derive(Clone, PartialEq)]
pub struct ParsedDocument {
    pub kind: DocKind,
    pub units: Vec<TextUnit>,
    pub meta: DocMeta,
    /// One per PDF page, including pages that yield no unit. Empty for other kinds.
    pub pages: Vec<PageInfo>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PageInfo {
    /// 1-based.
    pub number: u32,
    pub needs_ocr: bool,
    pub has_tables: bool,
}

#[derive(Clone, PartialEq)]
pub struct DocMeta {
    pub title: Option<String>,
    pub created: Option<SystemTime>,
    pub modified: Option<SystemTime>,
}

#[derive(Clone, PartialEq)]
pub struct TextUnit {
    pub locator: Locator,
    pub text: String,
    /// Empty for TXT/MD/DOCX. PDF: one per word. Image and OCR'd pages: one per line.
    pub spans: Vec<Span>,
}

/// `char_start..char_end` are Unicode scalar (`char`) indices into `TextUnit::text`, not byte offsets.
/// `bbox` is `[x0, y0, x1, y1]` normalized 0..1 of the page or image, origin at the top left.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Span {
    pub char_start: u32,
    pub char_end: u32,
    pub bbox: [f32; 4],
}

#[derive(Clone, PartialEq)]
pub enum Locator {
    Page(u32),
    Section { heading_path: Vec<String> },
    Line(u32),
    Image,
}

pub trait Parser: Send + Sync {
    /// `ext` is lower-cased, without the dot.
    fn supports(&self, ext: &str) -> bool;
    fn parse(&self, path: &ScopedPath) -> Result<ParsedDocument, ParseError>;
}

/// A parser and the magic prefix its files must start with, if any.
type Entry = (Option<&'static [u8]>, Box<dyn Parser>);

pub struct ParserRegistry {
    parsers: Vec<Entry>,
}

impl Default for ParserRegistry {
    fn default() -> Self {
        Self::new()
    }
}

impl ParserRegistry {
    pub fn new() -> Self {
        Self {
            parsers: vec![
                (None, Box::new(TxtParser)),
                (None, Box::new(MarkdownParser)),
                (Some(ZIP_MAGIC), Box::new(DocxParser::default())),
            ],
        }
    }

    /// Adds a parser that needs outside resources (a PDF library, an OCR engine).
    pub fn with_parser(
        mut self,
        magic: Option<&'static [u8]>,
        parser: impl Parser + 'static,
    ) -> Self {
        self.parsers.push((magic, Box::new(parser)));
        self
    }

    /// `ext` is lower-cased, without the dot.
    pub fn supports(&self, ext: &str) -> bool {
        self.parsers.iter().any(|(_, p)| p.supports(ext))
    }

    pub fn parse(&self, path: &ScopedPath) -> Result<ParsedDocument, ParseError> {
        let ext = path
            .as_path()
            .extension()
            .and_then(|e| e.to_str())
            .map(str::to_ascii_lowercase)
            .ok_or(ParseError::UnsupportedType)?;
        let (magic, parser) = self
            .parsers
            .iter()
            .find(|(_, p)| p.supports(&ext))
            .ok_or(ParseError::UnsupportedType)?;
        if let Some(magic) = magic {
            let mut head = vec![0; magic.len()];
            let mut file = File::open(path.as_path()).map_err(ParseError::Io)?;
            match file.read_exact(&mut head) {
                Ok(()) if head == *magic => {}
                Ok(()) => return Err(ParseError::MagicMismatch),
                Err(e) if e.kind() == std::io::ErrorKind::UnexpectedEof => {
                    return Err(ParseError::MagicMismatch)
                }
                Err(e) => return Err(ParseError::Io(e)),
            }
        }
        parser.parse(path)
    }
}

/// Reads the whole file, at most `MAX_FILE_BYTES`, plus its fs dates.
pub(super) fn read_capped(path: &ScopedPath) -> Result<(Vec<u8>, DocMeta), ParseError> {
    let file = File::open(path.as_path()).map_err(ParseError::Io)?;
    let md = file.metadata().map_err(ParseError::Io)?;
    if md.len() > MAX_FILE_BYTES {
        return Err(ParseError::TooLarge);
    }
    let mut bytes = Vec::new();
    // The take also catches a file that grew after the stat.
    file.take(MAX_FILE_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(ParseError::Io)?;
    if bytes.len() as u64 > MAX_FILE_BYTES {
        return Err(ParseError::TooLarge);
    }
    let meta = DocMeta {
        title: None,
        created: md.created().ok(),
        modified: md.modified().ok(),
    };
    Ok((bytes, meta))
}

/// BOM, then UTF-16 by NUL bytes, then UTF-8, then CP949, then Windows-1252. `\r\n` becomes `\n`.
fn decode_text(bytes: &[u8]) -> String {
    let text = if let Some((enc, bom_len)) = Encoding::for_bom(bytes) {
        enc.decode_without_bom_handling(&bytes[bom_len..]).0
    } else if let Some(enc) = utf16_by_nuls(bytes) {
        // Before the UTF-8 check: ASCII-only UTF-16 is also valid UTF-8 full of NULs.
        enc.decode_without_bom_handling(bytes).0
    } else if let Ok(s) = std::str::from_utf8(bytes) {
        s.into()
    } else if let Some(s) = EUC_KR.decode_without_bom_handling_and_without_replacement(bytes) {
        s
    } else {
        WINDOWS_1252.decode_without_bom_handling(bytes).0
    };
    text.trim_start_matches('\u{feff}').replace("\r\n", "\n")
}

// shortcut: only text that is mostly ASCII is detected as BOM-less UTF-16; Hangul-only UTF-16 without a BOM falls through to CP949/1252.
fn utf16_by_nuls(bytes: &[u8]) -> Option<&'static Encoding> {
    let pairs = bytes.len() / 2;
    if pairs == 0 {
        return None;
    }
    let even = bytes
        .iter()
        .step_by(2)
        .take(pairs)
        .filter(|b| **b == 0)
        .count();
    let odd = bytes.iter().skip(1).step_by(2).filter(|b| **b == 0).count();
    if even * 2 > pairs {
        Some(UTF_16BE)
    } else if odd * 2 > pairs {
        Some(UTF_16LE)
    } else {
        None
    }
}

pub struct TxtParser;

impl Parser for TxtParser {
    fn supports(&self, ext: &str) -> bool {
        ext == "txt"
    }

    fn parse(&self, path: &ScopedPath) -> Result<ParsedDocument, ParseError> {
        let (bytes, meta) = read_capped(path)?;
        Ok(ParsedDocument {
            kind: DocKind::Txt,
            units: txt_units(&decode_text(&bytes)),
            meta,
            pages: Vec::new(),
        })
    }
}

/// One unit per blank-line-separated paragraph, located by its first line (1-based).
fn txt_units(text: &str) -> Vec<TextUnit> {
    let mut units = Vec::new();
    let mut lines: Vec<&str> = Vec::new();
    let mut start = 0;
    let mut flush = |start: usize, lines: &mut Vec<&str>| {
        let joined = lines.join("\n");
        let body = joined.trim_end();
        if !body.is_empty() {
            units.push(TextUnit {
                locator: Locator::Line(u32::try_from(start).unwrap_or(u32::MAX)),
                text: body.to_owned(),
                spans: Vec::new(),
            });
        }
        lines.clear();
    };
    for (i, line) in text.split('\n').enumerate() {
        if line.trim().is_empty() {
            flush(start, &mut lines);
        } else {
            if lines.is_empty() {
                start = i + 1;
            }
            lines.push(line);
        }
    }
    flush(start, &mut lines);
    units
}

/// Splits text into units at headings, shared by Markdown and DOCX.
#[derive(Default)]
struct Sections {
    stack: Vec<(u8, String)>,
    body: String,
    units: Vec<TextUnit>,
}

impl Sections {
    fn flush(&mut self) {
        let text = self.body.trim();
        if !text.is_empty() {
            self.units.push(TextUnit {
                locator: Locator::Section {
                    heading_path: self.stack.iter().map(|(_, t)| t.clone()).collect(),
                },
                text: text.to_owned(),
                spans: Vec::new(),
            });
        }
        self.body.clear();
    }

    fn heading(&mut self, level: u8, text: &str) {
        let text = text.trim();
        if text.is_empty() {
            return;
        }
        self.flush();
        while self.stack.last().is_some_and(|(l, _)| *l >= level) {
            self.stack.pop();
        }
        self.stack.push((level, text.to_owned()));
    }

    fn finish(mut self) -> Vec<TextUnit> {
        self.flush();
        self.units
    }
}

pub struct MarkdownParser;

impl Parser for MarkdownParser {
    fn supports(&self, ext: &str) -> bool {
        matches!(ext, "md" | "markdown")
    }

    fn parse(&self, path: &ScopedPath) -> Result<ParsedDocument, ParseError> {
        let (bytes, mut meta) = read_capped(path)?;
        let (units, title) = md_units(&decode_text(&bytes));
        meta.title = title;
        Ok(ParsedDocument {
            kind: DocKind::Markdown,
            units,
            meta,
            pages: Vec::new(),
        })
    }
}

/// Units per heading section, and the first H1 as the title.
fn md_units(text: &str) -> (Vec<TextUnit>, Option<String>) {
    let mut s = Sections::default();
    let mut title = None;
    let mut heading: Option<(u8, String)> = None;
    for event in pulldown_cmark::Parser::new_ext(text, Options::ENABLE_TABLES) {
        let out = match &mut heading {
            Some((_, h)) => h,
            None => &mut s.body,
        };
        match event {
            MdEvent::Start(Tag::Heading { level, .. }) => {
                heading = Some((level as u8, String::new()));
            }
            MdEvent::End(TagEnd::Heading(level)) => {
                if let Some((_, h)) = heading.take() {
                    if level == HeadingLevel::H1 && title.is_none() && !h.trim().is_empty() {
                        title = Some(h.trim().to_owned());
                    }
                    s.heading(level as u8, &h);
                }
            }
            MdEvent::Text(t) | MdEvent::Code(t) => out.push_str(&t),
            MdEvent::SoftBreak => out.push(' '),
            MdEvent::HardBreak
            | MdEvent::End(TagEnd::Paragraph | TagEnd::Item | TagEnd::CodeBlock) => out.push('\n'),
            MdEvent::End(TagEnd::TableCell) => out.push('\t'),
            MdEvent::End(TagEnd::TableHead | TagEnd::TableRow) => {
                if out.ends_with('\t') {
                    out.pop();
                }
                out.push('\n');
            }
            _ => {}
        }
    }
    (s.finish(), title)
}

pub struct DocxParser {
    /// Total decompressed bytes read from all parts; declared sizes are never trusted.
    pub max_unzipped_bytes: u64,
}

impl Default for DocxParser {
    fn default() -> Self {
        Self {
            max_unzipped_bytes: 512 * 1024 * 1024,
        }
    }
}

impl Parser for DocxParser {
    fn supports(&self, ext: &str) -> bool {
        ext == "docx"
    }

    fn parse(&self, path: &ScopedPath) -> Result<ParsedDocument, ParseError> {
        // shortcut: DOCX docProps/core.xml dates are ignored; created/modified come from the file system.
        let (bytes, mut meta) = read_capped(path)?;
        let mut zip = ZipArchive::new(Cursor::new(bytes))?;
        let mut budget = self.max_unzipped_bytes;
        let doc = read_entry(&mut zip, "word/document.xml", &mut budget)?
            .ok_or(ParseError::Corrupt("missing word/document.xml"))?;
        let styles = match read_entry(&mut zip, "word/styles.xml", &mut budget)? {
            Some(xml) => heading_styles(&xml)?,
            None => Vec::new(),
        };
        if let Some(core) = read_entry(&mut zip, "docProps/core.xml", &mut budget)? {
            meta.title = core_title(&core)?;
        }
        let (units, has_tables) = docx_units(&doc, &styles)?;
        Ok(ParsedDocument {
            kind: DocKind::Docx { has_tables },
            units,
            meta,
            pages: Vec::new(),
        })
    }
}

/// Reads one part against the remaining decompressed budget. `None` if the part is absent.
fn read_entry<R: Read + Seek>(
    zip: &mut ZipArchive<R>,
    name: &str,
    budget: &mut u64,
) -> Result<Option<String>, ParseError> {
    let entry = match zip.by_name(name) {
        Ok(e) => e,
        Err(ZipError::FileNotFound) => return Ok(None),
        Err(e) => return Err(e.into()),
    };
    let mut buf = Vec::new();
    entry
        .take(budget.saturating_add(1))
        .read_to_end(&mut buf)
        .map_err(ParseError::Io)?;
    let len = buf.len() as u64;
    if len > *budget {
        return Err(ParseError::DecompressedTooLarge);
    }
    *budget -= len;
    // shortcut: XML parts must be UTF-8 (what Word writes); UTF-16 parts are rejected as Corrupt.
    let xml = String::from_utf8(buf).map_err(|_| ParseError::Corrupt("XML part is not UTF-8"))?;
    Ok(Some(xml.trim_start_matches('\u{feff}').to_owned()))
}

fn attr(e: &BytesStart, local: &str) -> Result<Option<String>, ParseError> {
    for a in e.attributes() {
        let a = a.map_err(|_| ParseError::Corrupt("malformed XML attribute"))?;
        if a.key.local_name().as_ref() == local {
            return Ok(Some(
                a.normalized_value(XmlVersion::Implicit1_0)?.into_owned(),
            ));
        }
    }
    Ok(None)
}

/// "heading 1", "Heading1", "HEADING 2" -> level.
fn heading_level(name: &str) -> Option<u8> {
    let n: u8 = name
        .trim()
        .to_ascii_lowercase()
        .strip_prefix("heading")?
        .trim()
        .parse()
        .ok()?;
    (1..=9).contains(&n).then_some(n)
}

/// styleId -> heading level, from each `w:style` whose `w:name` is "heading N".
fn heading_styles(xml: &str) -> Result<Vec<(String, u8)>, ParseError> {
    let mut out = Vec::new();
    let mut id: Option<String> = None;
    let mut r = Reader::from_str(xml);
    loop {
        match r.read_event()? {
            XmlEvent::Eof => break,
            XmlEvent::Start(e) | XmlEvent::Empty(e) => match e.local_name().as_ref() {
                "style" => id = attr(&e, "styleId")?,
                "name" => {
                    let level = attr(&e, "val")?.as_deref().and_then(heading_level);
                    if let (Some(id), Some(level)) = (&id, level) {
                        out.push((id.clone(), level));
                    }
                }
                _ => {}
            },
            XmlEvent::End(e) if e.local_name().as_ref() == "style" => id = None,
            _ => {}
        }
    }
    Ok(out)
}

/// `dc:title`, if present and non-empty.
fn core_title(xml: &str) -> Result<Option<String>, ParseError> {
    let mut r = Reader::from_str(xml);
    let mut title: Option<String> = None;
    loop {
        match r.read_event()? {
            XmlEvent::Eof => break,
            XmlEvent::Start(e) if e.local_name().as_ref() == "title" => title = Some(String::new()),
            XmlEvent::Text(t) => {
                if let Some(s) = &mut title {
                    s.push_str(&t.xml10_content());
                }
            }
            XmlEvent::GeneralRef(g) => {
                if let Some(s) = &mut title {
                    push_ref(s, &g)?;
                }
            }
            XmlEvent::End(e) if e.local_name().as_ref() == "title" => break,
            _ => {}
        }
    }
    Ok(title.map(|t| t.trim().to_owned()).filter(|t| !t.is_empty()))
}

fn push_ref(out: &mut String, g: &quick_xml::events::BytesRef) -> Result<(), ParseError> {
    if let Some(c) = g.resolve_char_ref()? {
        out.push(c);
        return Ok(());
    }
    let name = g.xml10_content();
    let s = quick_xml::escape::resolve_predefined_entity(&name)
        .ok_or(ParseError::Corrupt("unknown XML entity"))?;
    out.push_str(s);
    Ok(())
}

/// Body text of `word/document.xml` as heading sections, and whether it has any table.
fn docx_units(xml: &str, styles: &[(String, u8)]) -> Result<(Vec<TextUnit>, bool), ParseError> {
    let level_of = |id: &str| {
        styles
            .iter()
            .find(|(s, _)| s == id)
            .map(|(_, l)| *l)
            .or_else(|| heading_level(id))
    };
    let mut s = Sections::default();
    let mut has_tables = false;
    let mut para = String::new();
    let mut para_level: Option<u8> = None;
    // Paragraphs nest inside text boxes; nested ones merge into the outer paragraph.
    let mut para_depth = 0u32;
    let (mut in_run, mut in_t) = (false, false);
    let mut tbl_depth = 0u32;
    let mut row: Vec<String> = Vec::new();
    let mut cell = String::new();

    let mut r = Reader::from_str(xml);
    loop {
        match r.read_event()? {
            XmlEvent::Eof => break,
            XmlEvent::Start(e) => match e.local_name().as_ref() {
                "p" => {
                    if para_depth == 0 {
                        para.clear();
                        para_level = None;
                    }
                    para_depth += 1;
                }
                "pStyle" => para_level = attr(&e, "val")?.as_deref().and_then(level_of),
                "r" => in_run = true,
                "t" => in_t = true,
                "tbl" => {
                    tbl_depth += 1;
                    has_tables = true;
                }
                _ => {}
            },
            XmlEvent::Empty(e) => match e.local_name().as_ref() {
                "pStyle" => para_level = attr(&e, "val")?.as_deref().and_then(level_of),
                // w:tab also defines tab stops in w:pPr; only a run's tab is text.
                "tab" if in_run => para.push('\t'),
                "br" | "cr" if in_run => para.push('\n'),
                _ => {}
            },
            XmlEvent::Text(t) if in_t => para.push_str(&t.xml10_content()),
            XmlEvent::GeneralRef(g) if in_t => push_ref(&mut para, &g)?,
            XmlEvent::End(e) => match e.local_name().as_ref() {
                "t" => in_t = false,
                "r" => in_run = false,
                "p" => {
                    para_depth = para_depth.saturating_sub(1);
                    if para_depth > 0 {
                        para.push(' ');
                    } else if tbl_depth > 0 {
                        let text = para.trim();
                        if !text.is_empty() {
                            if !cell.is_empty() {
                                cell.push(' ');
                            }
                            cell.push_str(text);
                        }
                    } else if let Some(level) = para_level {
                        s.heading(level, &para);
                    } else {
                        s.body.push_str(&para);
                        s.body.push('\n');
                    }
                }
                "tc" if tbl_depth == 1 => row.push(std::mem::take(&mut cell)),
                // A nested table is flattened into the enclosing cell.
                "tc" | "tr" if tbl_depth > 1 => cell.push(' '),
                "tr" if tbl_depth == 1 => {
                    s.body.push_str(&row.join("\t"));
                    s.body.push('\n');
                    row.clear();
                }
                "tbl" => tbl_depth = tbl_depth.saturating_sub(1),
                _ => {}
            },
            _ => {}
        }
    }
    Ok((s.finish(), has_tables))
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use std::io::Write;
    use std::path::Path;

    pub(crate) fn fixture(bytes: &[u8], name: &str) -> (tempfile::TempDir, ScopedPath) {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join(name), bytes).unwrap();
        let p = ScopedPath::new(dir.path(), Path::new(name)).unwrap();
        (dir, p)
    }

    fn parse(bytes: &[u8], name: &str) -> Result<ParsedDocument, ParseError> {
        let (_d, p) = fixture(bytes, name);
        ParserRegistry::new().parse(&p)
    }

    fn line(n: u32, text: &str) -> (Locator, String) {
        (Locator::Line(n), text.to_owned())
    }

    fn sec(path: &[&str], text: &str) -> (Locator, String) {
        let heading_path = path.iter().map(|s| s.to_string()).collect();
        (Locator::Section { heading_path }, text.to_owned())
    }

    fn units(doc: &ParsedDocument) -> Vec<(Locator, String)> {
        assert!(doc.units.iter().all(|u| u.spans.is_empty()));
        doc.units
            .iter()
            .map(|u| (u.locator.clone(), u.text.clone()))
            .collect()
    }

    fn zip_bytes(files: &[(&str, &str)]) -> Vec<u8> {
        let mut w = zip::ZipWriter::new(Cursor::new(Vec::new()));
        for (name, body) in files {
            w.start_file(*name, zip::write::SimpleFileOptions::default())
                .unwrap();
            w.write_all(body.as_bytes()).unwrap();
        }
        w.finish().unwrap().into_inner()
    }

    const W: &str = r#"xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main""#;

    fn document(body: &str) -> String {
        format!(
            r#"<?xml version="1.0" encoding="UTF-8"?><w:document {W}><w:body>{body}</w:body></w:document>"#
        )
    }

    fn p(style: Option<&str>, text: &str) -> String {
        let ppr = style
            .map(|s| format!(r#"<w:pPr><w:pStyle w:val="{s}"/><w:tabs><w:tab w:val="left" w:pos="720"/></w:tabs></w:pPr>"#))
            .unwrap_or_default();
        format!("<w:p>{ppr}<w:r><w:t>{text}</w:t></w:r></w:p>")
    }

    fn table(rows: &[&[&str]]) -> String {
        let rows: String = rows
            .iter()
            .map(|cells| {
                let tcs: String = cells
                    .iter()
                    .map(|c| format!("<w:tc>{}</w:tc>", p(None, c)))
                    .collect();
                format!("<w:tr>{tcs}</w:tr>")
            })
            .collect();
        format!("<w:tbl>{rows}</w:tbl>")
    }

    #[test]
    fn txt_utf8_bom_mixed_korean_english() {
        let text = "\u{feff}안녕하세요 Hello\r\n둘째 줄 second\r\n\r\n\r\n셋째 Third  \r\n";
        let doc = parse(text.as_bytes(), "a.txt").unwrap();
        assert!(doc.kind == DocKind::Txt);
        assert!(doc.meta.title.is_none() && doc.meta.modified.is_some());
        assert!(
            units(&doc)
                == vec![
                    line(1, "안녕하세요 Hello\n둘째 줄 second"),
                    line(5, "셋째 Third"),
                ]
        );
    }

    #[test]
    fn txt_cp949_is_decoded() {
        let original = "한글 문서입니다. 똠방각하";
        let (bytes, _, unmappable) = EUC_KR.encode(original);
        assert!(!unmappable && std::str::from_utf8(&bytes).is_err());
        let doc = parse(&bytes, "k.txt").unwrap();
        assert!(units(&doc) == vec![line(1, original)]);
    }

    #[test]
    fn txt_utf16le_bom_is_decoded() {
        let mut bytes = vec![0xFF, 0xFE];
        bytes.extend(
            "유니코드 UTF-16\r\nline two"
                .encode_utf16()
                .flat_map(u16::to_le_bytes),
        );
        let doc = parse(&bytes, "u.txt").unwrap();
        assert!(units(&doc) == vec![line(1, "유니코드 UTF-16\nline two")]);
    }

    #[test]
    fn txt_utf16le_without_bom_is_detected() {
        let bytes: Vec<u8> = "plain ascii"
            .encode_utf16()
            .flat_map(u16::to_le_bytes)
            .collect();
        let doc = parse(&bytes, "u.txt").unwrap();
        assert!(units(&doc) == vec![line(1, "plain ascii")]);
    }

    #[test]
    fn md_heading_paths() {
        let md = "intro before\n\n# A\n\nbody a\n\n## B\n\nbody b *em* `code`\nsoft\n\n### C\n\n- one\n- two\n\n## D\n\n| h1 | h2 |\n|---|---|\n| x | y |\n";
        let doc = parse(md.as_bytes(), "n.md").unwrap();
        assert!(doc.kind == DocKind::Markdown);
        assert!(doc.meta.title.as_deref() == Some("A"));
        assert!(
            units(&doc)
                == vec![
                    sec(&[], "intro before"),
                    sec(&["A"], "body a"),
                    sec(&["A", "B"], "body b em code soft"),
                    sec(&["A", "B", "C"], "one\ntwo"),
                    sec(&["A", "D"], "h1\th2\nx\ty"),
                ]
        );
    }

    #[test]
    fn docx_headings_and_table() {
        let body = [
            p(Some("Heading1"), "Intro"),
            p(None, "Hello &amp; world"),
            p(Some("Heading2"), "Details"),
            "<w:p><w:r><w:t>a</w:t><w:tab/><w:t>b</w:t><w:br/><w:t>c</w:t></w:r></w:p>".into(),
            table(&[&["r1c1", "r1c2"], &["r2c1", "r2c2"]]),
            p(Some("Heading1"), "Next"),
            p(None, "last"),
        ]
        .concat();
        let core = r#"<cp:coreProperties xmlns:cp="c" xmlns:dc="http://purl.org/dc/elements/1.1/"><dc:title>My Title</dc:title></cp:coreProperties>"#;
        let bytes = zip_bytes(&[
            ("word/document.xml", &document(&body)),
            ("docProps/core.xml", core),
        ]);
        let doc = parse(&bytes, "d.docx").unwrap();
        assert!(doc.kind == DocKind::Docx { has_tables: true });
        assert!(doc.meta.title.as_deref() == Some("My Title"));
        assert!(
            units(&doc)
                == vec![
                    sec(&["Intro"], "Hello & world"),
                    sec(&["Intro", "Details"], "a\tb\nc\nr1c1\tr1c2\nr2c1\tr2c2"),
                    sec(&["Next"], "last"),
                ]
        );
    }

    #[test]
    fn docx_localized_heading_style_ids() {
        let styles = format!(
            r#"<w:styles {W}><w:style w:type="paragraph" w:styleId="1"><w:name w:val="heading 1"/></w:style><w:style w:type="paragraph" w:styleId="2"><w:name w:val="heading 2"/></w:style><w:style w:type="paragraph" w:styleId="a"><w:name w:val="Normal"/></w:style></w:styles>"#
        );
        let body = [
            p(Some("1"), "제목"),
            p(Some("a"), "본문"),
            p(Some("2"), "소제목"),
            p(None, "내용"),
        ]
        .concat();
        let bytes = zip_bytes(&[
            ("word/document.xml", &document(&body)),
            ("word/styles.xml", &styles),
        ]);
        let doc = parse(&bytes, "k.DOCX").unwrap();
        assert!(doc.kind == DocKind::Docx { has_tables: false });
        assert!(doc.meta.title.is_none());
        assert!(units(&doc) == vec![sec(&["제목"], "본문"), sec(&["제목", "소제목"], "내용")]);
    }

    #[test]
    fn docx_corrupt_is_an_error() {
        let mut garbage = ZIP_MAGIC.to_vec();
        garbage.extend_from_slice(&[0xAB; 300]);
        assert!(parse(&garbage, "g.docx").is_err());

        let valid = zip_bytes(&[("word/document.xml", &document(&p(None, "x")))]);
        for cut in [valid.len() / 2, valid.len() - 10] {
            assert!(parse(&valid[..cut], "t.docx").is_err());
        }

        let bad_xml = zip_bytes(&[("word/document.xml", "<w:document><w:body></w:p>")]);
        assert!(matches!(parse(&bad_xml, "x.docx"), Err(ParseError::Xml(_))));

        let missing = zip_bytes(&[("other.xml", "<a/>")]);
        assert!(matches!(
            parse(&missing, "m.docx"),
            Err(ParseError::Corrupt(_))
        ));
    }

    #[test]
    fn docx_zip_bomb_is_rejected() {
        let big = document(&p(None, &"z".repeat(64 * 1024)));
        let bytes = zip_bytes(&[("word/document.xml", &big)]);
        assert!((bytes.len() as u64) < 4096, "fixture should compress well");
        let (_d, path) = fixture(&bytes, "b.docx");
        let parser = DocxParser {
            max_unzipped_bytes: 4096,
        };
        assert!(matches!(
            parser.parse(&path),
            Err(ParseError::DecompressedTooLarge)
        ));
        let roomy = DocxParser {
            max_unzipped_bytes: 1024 * 1024,
        };
        assert!(roomy.parse(&path).is_ok());
    }

    #[test]
    fn file_over_cap_is_too_large() {
        let (_d, path) = fixture(b"", "huge.txt");
        File::options()
            .write(true)
            .open(path.as_path())
            .unwrap()
            .set_len(MAX_FILE_BYTES + 1)
            .unwrap();
        assert!(matches!(
            ParserRegistry::new().parse(&path),
            Err(ParseError::TooLarge)
        ));
    }

    #[test]
    fn registry_dispatch() {
        assert!(matches!(
            parse(b"plain text", "fake.docx"),
            Err(ParseError::MagicMismatch)
        ));
        assert!(matches!(
            parse(b"PK", "short.docx"),
            Err(ParseError::MagicMismatch)
        ));
        assert!(matches!(
            parse(b"x", "a.pdfx"),
            Err(ParseError::UnsupportedType)
        ));
        assert!(matches!(
            parse(b"x", "noext"),
            Err(ParseError::UnsupportedType)
        ));
        let doc = parse(b"upper", "UP.TXT").unwrap();
        assert!(units(&doc) == vec![line(1, "upper")]);
    }
}
