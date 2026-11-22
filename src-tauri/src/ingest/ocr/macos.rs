//! Apple Vision text recognition. The image goes from memory to a CGImage; nothing touches disk.

use objc2::rc::autoreleasepool;
use objc2::runtime::AnyObject;
use objc2::AllocAnyThread;
use objc2_core_foundation::CFData;
use objc2_core_graphics::{
    CGBitmapInfo, CGColorRenderingIntent, CGColorSpace, CGDataProvider, CGImage, CGImageAlphaInfo,
};
use objc2_foundation::{NSArray, NSDictionary, NSString};
use objc2_vision::{
    VNImageOption, VNImageRequestHandler, VNRecognizeTextRequest, VNRequest,
    VNRequestTextRecognitionLevel,
};

use super::{OcrEngine, OcrError, OcrLang, OcrLine};

pub struct VisionOcr;

impl OcrEngine for VisionOcr {
    fn recognize(
        &self,
        image: &image::RgbaImage,
        langs: &[OcrLang],
    ) -> Result<Vec<OcrLine>, OcrError> {
        let (w, h) = (image.width() as usize, image.height() as usize);
        if w == 0 || h == 0 {
            return Err(OcrError::BadImage);
        }
        autoreleasepool(|_| {
            let data = CFData::from_bytes(image.as_raw());
            let provider = CGDataProvider::with_cf_data(Some(&data))
                .ok_or(OcrError::Engine("CGDataProvider"))?;
            let space = CGColorSpace::new_device_rgb().ok_or(OcrError::Engine("CGColorSpace"))?;
            // SAFETY: `decode` may be null; size, 8 bits per component, 32 bits per pixel and the
            // row stride describe the tightly packed RGBA8 buffer that `provider` owns a copy of.
            let cg = unsafe {
                CGImage::new(
                    w,
                    h,
                    8,
                    32,
                    w * 4,
                    Some(&space),
                    CGBitmapInfo(CGImageAlphaInfo::Last.0),
                    Some(&provider),
                    std::ptr::null(),
                    false,
                    CGColorRenderingIntent::RenderingIntentDefault,
                )
            }
            .ok_or(OcrError::Engine("CGImage"))?;

            let request = VNRecognizeTextRequest::new();
            request.setRecognitionLevel(VNRequestTextRecognitionLevel::Accurate);
            request.setUsesLanguageCorrection(true);
            let langs: Vec<_> = langs
                .iter()
                .map(|l| NSString::from_str(l.bcp47()))
                .collect();
            request.setRecognitionLanguages(&NSArray::from_retained_slice(&langs));

            let options = NSDictionary::<VNImageOption, AnyObject>::new();
            // SAFETY: `options` is an empty dictionary of the declared key and value types.
            let handler = unsafe {
                VNImageRequestHandler::initWithCGImage_options(
                    VNImageRequestHandler::alloc(),
                    &cg,
                    &options,
                )
            };
            let base: &VNRequest = &request;
            handler
                .performRequests_error(&NSArray::from_slice(&[base]))
                .map_err(|_| OcrError::Engine("Vision request"))?;

            let mut lines = Vec::new();
            let Some(results) = request.results() else {
                return Ok(lines);
            };
            for obs in results.iter() {
                let Some(top) = obs.topCandidates(1).firstObject() else {
                    continue;
                };
                // SAFETY: a plain struct getter on a live observation.
                let r = unsafe { obs.boundingBox() };
                let (x, y, bw, bh) = (r.origin.x, r.origin.y, r.size.width, r.size.height);
                // Vision's origin is bottom left; flip y to our top-left convention.
                let bbox = [x, 1.0 - (y + bh), x + bw, 1.0 - y].map(|v| v.clamp(0.0, 1.0) as f32);
                lines.push(OcrLine {
                    text: top.string().to_string(),
                    bbox,
                });
            }
            Ok(lines)
        })
    }
}
