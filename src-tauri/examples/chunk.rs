//! Prints a file's chunks as JSON: `cargo run --example chunk -- <file>`.

use std::error::Error;
use std::path::Path;
use std::sync::Arc;

use locite_lib::ingest::chunker::{chunk, CharApprox, ChunkerConfig, TokenCounter};
use locite_lib::ingest::image::{ImageParser, JPEG_MAGIC, PNG_MAGIC};
use locite_lib::ingest::ocr::{self, OcrEngine};
use locite_lib::ingest::parser::ParserRegistry;
use locite_lib::ingest::pdf::{PdfParser, PDF_MAGIC};
use locite_lib::security::paths::ScopedPath;
use locite_lib::storage::db::DocId;

const PDFIUM: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/vendor/pdfium/lib/libpdfium.dylib"
);

fn main() {
    if let Err(e) = run() {
        eprintln!("error: {e}");
        std::process::exit(1);
    }
}

fn run() -> Result<(), Box<dyn Error>> {
    let arg = std::env::args().nth(1).ok_or("usage: chunk <file>")?;
    let file = Path::new(&arg).canonicalize()?;
    let root = file.parent().ok_or("file has no parent directory")?;
    let path = ScopedPath::new(root, &file)?;

    let ocr: Arc<dyn OcrEngine> = Arc::from(ocr::default_engine());
    let mut registry = ParserRegistry::new()
        .with_parser(Some(PNG_MAGIC), ImageParser::png(ocr.clone()))
        .with_parser(Some(JPEG_MAGIC), ImageParser::jpeg(ocr.clone()));
    match PdfParser::new(Path::new(PDFIUM), ocr) {
        Ok(p) => registry = registry.with_parser(Some(PDF_MAGIC), p),
        Err(e) => eprintln!("PDF support off: {e}"),
    }

    let doc = registry.parse(&path)?;
    let chunks = chunk(DocId(0), &doc, &ChunkerConfig::default(), &CharApprox);
    let out: Vec<_> = chunks
        .iter()
        .map(|c| {
            let sha: String = c.sha256.iter().map(|b| format!("{b:02x}")).collect();
            serde_json::json!({
                "chunk_index": c.chunk_index,
                "page": c.page,
                "section_path": c.section_path,
                "start_offset": c.start_offset,
                "end_offset": c.end_offset,
                "tokens": CharApprox.count(&c.text),
                "bbox": c.bbox,
                "sha256": sha,
                "text": c.text,
            })
        })
        .collect();
    println!("{}", serde_json::to_string_pretty(&out)?);
    Ok(())
}
