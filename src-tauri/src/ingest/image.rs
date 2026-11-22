//! PNG/JPEG -> `ParsedDocument` through OCR. Decoding stays in memory.

use std::io::Cursor;
use std::sync::Arc;

use image::{ImageFormat, ImageReader, Limits};

use super::error::ParseError;
use super::ocr::{self, OcrEngine, OcrLang};
use super::parser::{read_capped, DocKind, Locator, ParsedDocument, Parser, TextUnit};
use crate::security::paths::ScopedPath;

pub const PNG_MAGIC: &[u8] = b"\x89PNG\r\n\x1a\n";
pub const JPEG_MAGIC: &[u8] = b"\xFF\xD8\xFF";
/// Wider or taller images are rejected before decoding.
pub const MAX_IMAGE_SIDE: u32 = 8192;
/// Decoder allocation cap: one 8192 x 8192 RGBA image plus headroom.
const MAX_DECODE_BYTES: u64 = 300 * 1024 * 1024;

/// One format per parser, so the registry can check each format's magic.
pub struct ImageParser {
    format: ImageFormat,
    ocr: Arc<dyn OcrEngine>,
    pub langs: Vec<OcrLang>,
}

impl ImageParser {
    pub fn png(ocr: Arc<dyn OcrEngine>) -> Self {
        Self::with_format(ImageFormat::Png, ocr)
    }

    pub fn jpeg(ocr: Arc<dyn OcrEngine>) -> Self {
        Self::with_format(ImageFormat::Jpeg, ocr)
    }

    fn with_format(format: ImageFormat, ocr: Arc<dyn OcrEngine>) -> Self {
        Self {
            format,
            ocr,
            langs: vec![OcrLang::KoKr, OcrLang::EnUs],
        }
    }
}

impl Parser for ImageParser {
    fn supports(&self, ext: &str) -> bool {
        match self.format {
            ImageFormat::Png => ext == "png",
            _ => matches!(ext, "jpg" | "jpeg"),
        }
    }

    fn parse(&self, path: &ScopedPath) -> Result<ParsedDocument, ParseError> {
        let (bytes, meta) = read_capped(path)?;
        let mut limits = Limits::default();
        limits.max_image_width = Some(MAX_IMAGE_SIDE);
        limits.max_image_height = Some(MAX_IMAGE_SIDE);
        limits.max_alloc = Some(MAX_DECODE_BYTES);
        let mut reader = ImageReader::with_format(Cursor::new(bytes), self.format);
        reader.limits(limits);
        let image = reader.decode()?.to_rgba8();
        let (text, spans) = ocr::join_lines(self.ocr.recognize(&image, &self.langs)?);
        let units = if text.is_empty() {
            Vec::new()
        } else {
            vec![TextUnit {
                locator: Locator::Image,
                text,
                spans,
            }]
        };
        Ok(ParsedDocument {
            kind: DocKind::Image,
            units,
            meta,
            pages: Vec::new(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ingest::ocr::NoopOcr;
    use crate::ingest::parser::tests::fixture;
    use crate::ingest::parser::ParserRegistry;

    fn registry(ocr: Arc<dyn OcrEngine>) -> ParserRegistry {
        ParserRegistry::new()
            .with_parser(Some(PNG_MAGIC), ImageParser::png(ocr.clone()))
            .with_parser(Some(JPEG_MAGIC), ImageParser::jpeg(ocr))
    }

    fn encode(img: &image::RgbaImage, format: ImageFormat) -> Vec<u8> {
        let mut out = Cursor::new(Vec::new());
        image::DynamicImage::ImageRgba8(img.clone())
            .to_rgb8()
            .write_to(&mut out, format)
            .unwrap();
        out.into_inner()
    }

    #[test]
    fn image_bad_bytes_are_errors() {
        let img = image::RgbaImage::from_pixel(64, 64, image::Rgba([255, 255, 255, 255]));
        let png = encode(&img, ImageFormat::Png);
        let jpeg = encode(&img, ImageFormat::Jpeg);
        let r = registry(Arc::new(NoopOcr));
        let parse = |bytes: &[u8], name: &str| {
            let (_d, p) = fixture(bytes, name);
            r.parse(&p)
        };
        assert!(matches!(
            parse(&jpeg, "a.png"),
            Err(ParseError::MagicMismatch)
        ));
        assert!(matches!(
            parse(b"garbage", "b.png"),
            Err(ParseError::MagicMismatch)
        ));
        assert!(matches!(
            parse(&png, "c.jpg"),
            Err(ParseError::MagicMismatch)
        ));
        assert!(parse(&png[..png.len() / 2], "d.png").is_err());
        assert!(parse(&jpeg[..jpeg.len() / 2], "e.jpeg").is_err());
        let blank = parse(&png, "f.PNG").unwrap();
        assert!(blank.kind == DocKind::Image && blank.units.is_empty() && blank.pages.is_empty());
    }

    #[test]
    fn image_over_side_limit_is_rejected() {
        let wide = image::RgbaImage::new(MAX_IMAGE_SIDE + 1, 1);
        let (_d, p) = fixture(&encode(&wide, ImageFormat::Png), "w.png");
        assert!(matches!(
            registry(Arc::new(NoopOcr)).parse(&p),
            Err(ParseError::TooLarge)
        ));
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn png_is_recognized_with_vision() {
        let png = encode(&crate::ingest::pdf::tests::hello_image(), ImageFormat::Png);
        let (_d, p) = fixture(&png, "hello.png");
        let doc = registry(Arc::new(crate::ingest::ocr::VisionOcr))
            .parse(&p)
            .unwrap();
        assert_eq!(doc.units.len(), 1);
        assert!(doc.units[0].locator == Locator::Image);
        assert!(doc.units[0].text.contains(crate::brand::APP_NAME));
    }
}
