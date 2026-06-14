use objc2::rc::Retained;
use objc2::ClassType;
use objc2_foundation::{NSArray, NSDictionary, NSError, NSString, NSURL};
use objc2_vision::{
    VNImageRequestHandler, VNRecognizeTextRequest, VNRecognizedTextObservation, VNRequest,
    VNRequestTextRecognitionLevel,
};
use std::path::Path;

#[cfg(target_os = "macos")]
pub fn run_native_ocr(image_path: &Path) -> Result<String, String> {
    objc2::rc::autoreleasepool(|_| {
        let path_str = image_path
            .to_str()
            .ok_or_else(|| "Invalid image path".to_string())?;

        let path_ns = NSString::from_str(path_str);
        let url = unsafe { NSURL::fileURLWithPath_isDirectory(&path_ns, false) };

        // Vision expects &NSDictionary<NSString, AnyObject>
        let options = NSDictionary::new();

        let handler = unsafe {
            let h = VNImageRequestHandler::alloc();
            VNImageRequestHandler::initWithURL_options(h, &url, &options)
        };

        let request = unsafe {
            let req = VNRecognizeTextRequest::init(VNRecognizeTextRequest::alloc());
            req.setRecognitionLevel(VNRequestTextRecognitionLevel::Accurate);

            // Explicitly set languages to support Chinese
            let langs = NSArray::from_id_slice(&[
                NSString::from_str("zh-Hans"),
                NSString::from_str("en-US"),
            ]);
            req.setRecognitionLanguages(&langs);

            // Revision 3 is required for better Chinese support
            req.setRevision(3);

            req.setUsesLanguageCorrection(true);
            req
        };

        // We need an NSArray of VNRequest
        let requests =
            unsafe { NSArray::from_id_slice(&[Retained::cast::<VNRequest>(request.clone())]) };

        unsafe {
            let res: Result<(), Retained<NSError>> = handler.performRequests_error(&requests);
            res.map_err(|e| e.to_string())?;
        }

        let results = unsafe { request.results() };
        let results = match results {
            Some(res) => res,
            None => return Ok(String::new()),
        };

        let mut recognized_strings = Vec::new();

        for i in 0..results.count() {
            let observation = unsafe { results.objectAtIndex(i) };

            let observation: Retained<VNRecognizedTextObservation> =
                unsafe { Retained::cast::<VNRecognizedTextObservation>(observation) };

            let top_candidates = unsafe { observation.topCandidates(1) };
            if top_candidates.count() > 0 {
                let candidate = unsafe { top_candidates.objectAtIndex(0) };
                let text = unsafe { candidate.string() };
                recognized_strings.push(text.to_string());
            }
        }

        Ok(recognized_strings.join("\n"))
    })
}

#[cfg(not(target_os = "macos"))]
pub fn run_native_ocr(_image_path: &Path) -> Result<String, String> {
    Err("Native OCR is only supported on macOS".to_string())
}
